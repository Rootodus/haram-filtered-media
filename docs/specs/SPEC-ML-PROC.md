// Note: update this later by asking ai about the new code and what should be updated about the spec.

# MLProcessor Data Contract (Spike-01)
ID: SPEC-ML-PROC  
Status: STABLE-FOR-SPIKE  
Depends on: ARCH-REQ

## IPC Protocol [ANCHOR: PROTOCOL-SPIKE]
- Transport: Unix Domain Socket (Linux/macOS) or Named Pipe (Windows).
- Framing: `[Payload_Length: u32]` + `[MessagePack_Payload: bytes]`.
- Byte Order: Little-Endian (LE) for the length prefix AND all numerical fields.
- Backpressure (ACK): The `Runtime` SHALL send a single byte `0x01` (OK) back through the pipe to the `Loader` upon completion of the `STAGE-RENDER` for the current frame.

## ContentBuffer [ANCHOR: SCHEMA-BUFFER-SPIKE]
| Field | Type | Invariant |
| --- | --- | --- |
| `timestamp` | u64 | Monotonic acquisition time. |
| `width` | u32 | Must match native window width. |
| `height` | u32 | Must match native window height. |
| `pixel_data` | bin | MessagePack Binary type. Length MUST be `width * height * 4`. |

## ProcessedBuffer [ANCHOR: SCHEMA-OUTPUT-SPIKE]
| Field | Type | Description |
| --- | --- | --- |
| `instructions` | List<VisualAction> | Sequential list of render commands. |

### VisualAction [ANCHOR: WIRE-FORMAT]
The `VisualAction` is a fixed-size structure for predictable parsing.

- `action_type`: `u8`
  - `0` = `BLUR`
  - `1` = `BLACKBOX`
- `rect`: `[f32; 4]`
  - Layout: `[x, y, width, height]`
  - Unit: Pixel Coordinates (Relative to 0,0 top-left).
  - Float Format: IEEE 754 Single Precision.

## Invariants
- HEADER-PAYLOAD-SPLIT: The IPC protocol MUST separate metadata (MessagePack) from raw pixel data (Raw Bitstream). Do not wrap pixels in MessagePack; this prevents decoder-induced latency spikes.
- LITTLE-ENDIAN-ONLY: All binary fields (length prefixes, coordinate floats) MUST use Little-Endian encoding to match the target x86/ARM64 architectures.
- ACK-GATING: The 1-byte `0x01` ACK is the primary backpressure mechanism. Its placement determines system-wide latency.

## Operational Constraints
- Stop-and-Wait: The `Loader` MUST NOT initiate a new `STAGE-ACQUIRE` until it receives the `0x01` ACK for the previous frame.
- Deserialization: The `Runtime` SHALL use `serde_bytes` or equivalent to deserialize `pixel_data` directly into a borrowed slice `&[u8]` to avoid an intermediate `Vec<u8>` allocation.
