<!-- Full Path: /contract/CONTR-EXEC-BASE.md -->

# Execution Base Contract
ID: CONTR-EXEC-BASE  
Status: PRELIMINARY  
Depends on: STD-DOC, INT-DATA-MOD

## Scope Boundary
- This contract defines execution semantics only
- It MUST NOT define architecture constraints
- It MUST NOT define benchmarking interpretation rules

## Core Execution Unit
UnitOfWork:
- single input item processed from ingestion to final output write

InputType:
- ContentBuffer

OutputType:
- ContentBuffer

## Stage Definitions

### FetchStage
FetchStage(input: ContentBuffer) -> ContentBuffer

Rules:
- MUST perform input retrieval or pass-through only
- MUST NOT perform ML processing
- MUST NOT perform rendering logic
- MUST return valid ContentBuffer

Failure:
- MUST return FAIL status with valid ContentBuffer

### ProcessStage (MLProcessor)
ProcessStage(input: ContentBuffer) -> ContentBuffer

Rules:
- MUST transform payload only
- MUST NOT perform I/O operations
- MUST be stateless
- MUST be deterministic

### RenderStage
RenderStage(input: ContentBuffer) -> ContentBuffer

Rules:
- MUST serialize output
- MUST NOT modify semantics
- MUST NOT perform ML computation
- MAY simulate output delay

## Timing Rules

### StartTime
- Config B: immediately before FetchStage invocation
- Config A: immediately before enqueue into pipeline

### EndTime
- immediately after RenderStage returns

### Latency
- includes queue waiting time where applicable
- includes scheduling delays where applicable

## Execution Model

### Config A
- Fetch -> Process -> Render
- bounded queues between stages
- ordering preserved
- blocking on full queue (no dropping)

### Config B
- single call chain execution
- no queues
- no buffering

## Identity Constraint
Must be identical:
- stage logic
- dataset
- output schema

Only structure differs.

## Error Handling
- MUST return SUCCESS or FAIL
- MUST NOT throw unhandled exceptions
- FAIL MUST be logged

## Logging
config_id, input_id, iteration, start_time_ms, end_time_ms, latency_ms, status

## Non-variability
- identical hardware
- identical runtime
- identical dataset ordering
