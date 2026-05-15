# Performance Optimization Strategy
ID: ARCH-PERF-STRATEGY  
Status: STABLE  
Depends on: @ARCH-REQ, @ARCH-SYS-MAP

## Purpose
Defines the technical roadmap for minimizing latency and maximizing throughput beyond the baseline implementation.

## Serialization Evolution - PERF-SERIAL
| Stage | Format | Nature | Reason |
| --- | --- | --- | --- |
| Current | `FlatBuffers` | Memory-mapped | Provides zero-decode random access; solves 11 ms MessagePack scanning bottleneck. |
| Endgame | `FlatBuffers` + `SHM` | Shared Memory | Eliminates OS-level memory copies between Loader and Runtime. |

### FlatBuffers Implementation
- Status: ACTIVE.
- Evidence: @EXP-SPIKE-05-DOM-STRESS demonstrated that MessagePack scanning exceeded the 16.6 ms frame budget.
- Benefit: The `Extractor` and `MLProcessor` access DOM nodes and metadata via pointer offsets with zero CPU parsing overhead.

## Transport Optimization - PERF-TRANSPORT
Current IPC relies on TCP Loopback (Windows) or Unix Sockets (Linux).

### Shared Memory (SHM) - SHARED-MEM
- Strategy: Map a circular buffer in RAM accessible by both the `Loader` and `Runtime`.
- Constraint: Requires platform-specific logic (`shmem` on Unix, `CreateFileMapping` on Windows).
- Benefit: Reduces frame transfer latency to near-zero by eliminating kernel-space copies.

## ML Ingestion - PERF-ML
- Target: Eliminate the `Vec` to `Tensor` conversion.
- Strategy: Use FlatBuffers Structs for fixed-width numerical data (e.g., coordinates, style indices).
- Mechanism: The `Parser` stage reads aligned data directly from the memory-mapped buffer, matching the input shape of the `Inference Engine` without reshuffling.
- Note: Apache Arrow is REJECTED as it introduces unnecessary columnar-to-row overhead for single-page inference units.
