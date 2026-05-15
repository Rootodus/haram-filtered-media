# Experiment: Length-Prefixed IPC Bridge
ID: EXP-SPIKE-01-IPC-PROTO  
Status: SUCCESS  
Depends on: @STD-DOC, @EXP-RULES

## Hypothesis
A binary TCP loopback using a u32 length prefix can move MessagePack objects between Node.js and Rust without protocol desynchronization.

## Evidence
- Format: [Payload_Length: u32 LE] + [MessagePack: bytes].
- Result: Successfully transferred dummy `ContentBuffer` metadata.
- Discovery: Encountered type mismatch (JS Float64 vs Rust U64); resolved by ensuring JS-side integer precision.

## Conclusion
The framing protocol is stable and adopted for all subsequent spikes.
