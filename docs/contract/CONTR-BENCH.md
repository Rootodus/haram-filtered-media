<!-- Full Path: /contract/CONTR-BENCH.md -->

# Benchmark Contract
ID: CONTR-BENCH  
Status: PRELIMINARY  
Depends on: STD-DOC

## Scope Boundary
- This contract defines benchmarking requirements only
- It MUST NOT define execution semantics (see CONTR-EXEC-BASE)
- It MUST NOT define architecture constraints (see CONTR-ARCH)

## Benchmark Objective
- Compare Config A (staged pipeline) vs Config B (single-stage execution)
- Comparison MUST be based on identical inputs and identical measurement rules

## Measurement invariants
- All timing definitions MUST be taken from CONTR-EXEC-BASE
- No alternative latency definitions are permitted here
- No stage-level timing is permitted

## Output requirement
- Benchmark MUST produce reproducible logs under identical dataset and environment
- Results MUST be comparable across runs without reinterpretation

## System metrics (observational only)
- throughput_items_per_sec MAY be computed post-run
- total_runtime_ms MAY be derived from logs
- failure counts MAY be aggregated post-run

## Dataset constraint
- Dataset MUST be identical across configurations
- Dataset MUST be pre-generated before execution begins
- Dataset MUST remain immutable during execution

## Execution constraint
- Config selection MUST NOT alter dataset or measurement rules
- No runtime adaptation is allowed based on observed performance

## Output artifacts
- EXP-ARCH-BASE-RUN-A.log
- EXP-ARCH-BASE-RUN-B.log

## Interpretation boundary
- This contract does not interpret results
- This contract does not define performance expectations
