# MLProcessor Data Contract
ID: SPEC-ML-PROC  
Status: STABLE-FOR-SPIKE  
Depends on: ARCH-REQ, ARCH-PERF-STRATEGY, STYLE-RUST

## IPC Protocol [Anchor: PROTOCOL-SPIKE]
- Transport: TCP Loopback (127.0.0.1) [Windows] or Unix Domain Sockets [Unix].
- Framing: `[FB_Length: u32]` + `[FlatBuffer_Payload: bytes]` + `[Raw_Pixels: bytes]`.
- Byte Order: Little-Endian (LE) for length prefixes and numerical data.
- Backpressure (ACK): The `Runtime` SHALL send a single byte `0x01` back to the `Loader` ONLY AFTER `surface_texture.present()` has completed for the frame.

## ContentBuffer [Anchor: SCHEMA-BUFFER-SPIKE]
The `ContentBuffer` is composed of a memory-mapped FlatBuffer and a trailing pixel bitstream.

### FlatBuffer Structure (`schema.fbs`)
```flatbuffers
struct Vec4 { x: float; y: float; w: float; h: float; }

table DomNode {
    id: uint32;
    tag: string;
    text: string;
    rect: Vec4;
}

table Metadata {
    timestamp: uint64;
    width: uint32;
    height: uint32;
    nodes: [DomNode];
}

root_type Metadata;
```

### Pixel Payload
- Format: Raw RGBA8.
- Size: `Metadata.width * Metadata.height * 4` bytes.
- Position: Immediate successor to the FlatBuffer bytes.

## ProcessedBuffer [Anchor: SCHEMA-OUTPUT-SPIKE]
| Field | Type | Description |
| --- | --- | --- |
| `instructions` | List<VisualAction> | Sequential list of render commands. |

### VisualAction [Anchor: WIRE-FORMAT]
The `VisualAction` is a fixed-size structure for predictable parsing.

- `action_type`: `u8`
  - `0` = `BLUR`
  - `1` = `BLACKBOX`
- `rect`: `[f32; 4]`
  - Layout: `[x, y, width, height]`
  - Unit: Pixel Coordinates (Relative to 0,0 top-left).
  - Float Format: IEEE 754 Single Precision.

## Invariants
- ZERO-DECODE-DOM: The system MUST NOT use MessagePack or JSON for DOM data. Access to nodes MUST be performed via FlatBuffer pointer offsets to eliminate the 11ms scanning bottleneck observed in `Spike-05`.
- FB-PIXEL-SPLIT: The FlatBuffer payload contains structural metadata only. Raw pixel data MUST remain as a trailing bitstream to prevent FlatBuffer builder overhead for large binary blobs.
- LIFETIME-STRICT: The `Runtime` SHALL treat the FlatBuffer as a read-only memory map. Strings (`tag`, `text`) are accessed as `&str` directly from the IPC buffer.

## Operational Constraints
- Stop-and-Wait: The `Loader` MUST NOT initiate a new capture until the `0x01` ACK is received, ensuring the system is clocked to the physical GPU presentation rate.
- Zero-Copy Hand-off: The `Parser` stage SHALL verify the FlatBuffer integrity without copying. The `MLProcessor` SHALL receive a pointer into the existing IPC buffer.
