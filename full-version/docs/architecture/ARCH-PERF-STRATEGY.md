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

## Parallel ML Execution - PERF-PARALLEL
- Target: Avoid sequential latency accumulation when multiple ML models are active on the same frame.
- Strategy: Execute each model in parallel on Tokio's blocking thread pool (`spawn_blocking`), one task per model.
- Mechanism: The render thread waits for all tasks to complete before presenting. Outputs are concatenated.
- Benefit: Total inference latency becomes `max(model_latencies)` instead of `sum(model_latencies)`, provided sufficient CPU cores.
- Constraint: GPU‑bound models (DirectML/CUDA) cannot run in parallel due to device contention; they must be serialized or use CPU fallback.
- Rationale: Parallelism leverages multi‑core CPUs while preserving hard‑sync ACK and zero‑copy data passing.
