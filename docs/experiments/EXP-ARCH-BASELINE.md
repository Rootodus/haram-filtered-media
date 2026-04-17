# Architecture Baseline Experiment
ID: EXP-ARCH-BASELINE  
Status: RAW  
Depends on: STD-DOC, SPEC-PIPELINE, SPEC-BENCHMARK-RULES

## Purpose
Compare Configuration A [Staged Pipeline] vs Configuration B [Single-Stage] under identical inputs.

## Unit Definition
UnitOfWork:
- Single `ContentBuffer` processed from ingestion to completion.

Execution semantics:
- MUST follow `SPEC-PIPELINE`.

## Dataset
Type: Synthetic fixed set.  
Structure:
- `input_id`: String.
- `payload_type`: Enum (Text, ImageStub).
- `payload_size`: Integer.

Rules:
- Dataset MUST be pre-generated before execution.
- Dataset MUST remain immutable during execution.

Repetitions:
- EACH input processed 1000 times per configuration.

## Config A — Staged Pipeline
Topology:
- `FetchStage` -> Bounded Queue -> `ProcessStage` -> Bounded Queue -> `RenderStage`.

Communication:
- Asynchronous bounded channels.

## Config B — Single-Stage Pipeline
Topology:
- `FetchStage` -> `ProcessStage` -> `RenderStage` as a synchronous call chain.

## Outputs
Artifacts:
- `EXP-ARCH-BASE-RUN-A.log`
- `EXP-ARCH-BASE-RUN-B.log`

Logging format:
- `config_id`, `input_id`, `iteration`, `start_time_ms`, `end_time_ms`, `latency_ms`, `status`.

## Notes / Explanatory
- [EXPLANATORY] Timing rules AND measurement invariants are defined in `SPEC-BENCHMARK-RULES`.
- [EXPLANATORY] This record provides the evidence for architectural decisions in `LOG-DECISIONS`.
