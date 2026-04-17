# Benchmark Rules
ID: SPEC-BENCHMARK-RULES  
Status: STABLE  
Depends on: STD-DOC, SPEC-SYSTEM-RULES

## Measurement Invariants

### Latency Definition
Constraint:
- `Latency` MUST include queue waiting time AND scheduling delays.
- `StartTime` for Configuration A MUST be recorded immediately before enqueue into the pipeline.
- `StartTime` for Configuration B MUST be recorded immediately before `FetchStage` invocation.
- `EndTime` MUST be recorded immediately after `RenderStage` returns.

Rationale:
- Consistent timing points ensure fair comparison between asynchronous AND synchronous execution models.

### Metric Computation
Constraint:
- `throughput_items_per_sec` MUST be computed post-run.
- `total_runtime_ms` MUST be derived from the logs ONLY.
- Stage-level timing is PROHIBITED to prevent instrumentation bias.

## Dataset Constraints
Constraint:
- The dataset MUST be identical across Configuration A AND Configuration B.
- The dataset MUST be pre-generated before execution begins.
- The dataset MUST remain immutable during the entire execution run.

Rationale:
- Dataset stability ensures that variability in results is attributable to the system architecture ONLY.

## Execution Requirements
Constraint:
- Benchmark MUST NOT perform runtime adaptation based on observed performance.
- Identical hardware AND software runtimes MUST be used for all configurations.
- Results MUST be logged as `EXP-ARCH-BASE-RUN-A.log` AND `EXP-ARCH-BASE-RUN-B.log`.

## Notes / Explanatory
- [EXPLANATORY] This document provides the constraints for valid scientific comparison.
- [EXPLANATORY] These rules do not define performance expectations, only measurement integrity.
