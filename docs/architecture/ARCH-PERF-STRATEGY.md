# Performance Optimization Strategy
ID: ARCH-PERF-STRATEGY  
Status: DRAFT  
Depends on: ARCH-REQ, ARCH-SYS-MAP

## Purpose
Defines the technical roadmap for minimizing latency and maximizing throughput beyond the baseline implementation.

## Serialization Evolution [Anchor: PERF-SERIAL]
The system utilizes a tiered approach to data serialization to balance development speed with execution performance.

| Stage | Format | Nature | Reason |
| --- | --- | --- | --- |
| Current | `MessagePack` | Object-based | Fastest to code for the initial Spike. |
| Endgame | `FlatBuffers` | Memory-mapped | Used for DOM, Styles, and Metadata. Allows the CPU to "peek" at any node with zero-copy. |

### FlatBuffers Transition
- Trigger: When STAGE-TRANSFORM latency for DOM trees exceeds 5 ms.
- Benefit: Allows the `Extractor` to "peek" at specific element properties without walking or decoding the entire MessagePack map.

## Transport Optimization [Anchor: PERF-TRANSPORT]
Current IPC relies on TCP Loopback (Windows) or Unix Sockets (Linux).

### Shared Memory (SHM) [Anchor: FUTURE]
- Strategy: Map a circular buffer in RAM accessible by both the `Loader` and `Runtime`.
- Constraint: Requires platform-specific logic (`shmem` on Unix, `CreateFileMapping` on Windows).
- Benefit: Reduces frame transfer latency to near-zero by eliminating kernel-space copies.

## ML Ingestion [Anchor: PERF-ML]
- Target: Eliminate the `Vec` to `Tensor` conversion.
- Strategy: Use Columnar Layouts (Arrow) for CSS styles.
- Mechanism: The `Parser` stage writes styles into contiguous memory blocks that match the input shape of the `Inference Engine`.
