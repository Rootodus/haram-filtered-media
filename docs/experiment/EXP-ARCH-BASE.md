# Architecture Baseline Experiment
ID: EXP-ARCH-BASE  
Status: RAW  
Depends on: CONTR-ARCH, INT-ARCH

## Purpose
Compare runtime performance of two execution models:
- Config A: staged pipeline
- Config B: single-stage pipeline

Comparison is based on identical inputs and identical measurement rules.

## Execution Unit Definition
UnitOfWork:
- one input item processed from start to completion

StartTime:
- timestamp when input enters system boundary

EndTime:
- timestamp when output is fully produced and written to output log

Latency:
- EndTime - StartTime (ms)

Throughput:
- total completed units / total runtime seconds

## Input Dataset
DatasetType: synthetic deterministic set

Structure:
- input_id: string
- payload_type: enum(Text, ImageStub)
- payload_size: integer (bytes or characters)

Rules:
- dataset MUST be identical for both configs
- dataset MUST be preloaded before execution starts
- dataset MUST NOT be modified during execution

Repetitions:
- each input MUST be processed exactly 1000 times per config

## Config A — Staged Pipeline
PipelineStages (strict order):
1. FetchStage
2. ProcessStage (MLProcessor)
3. RenderStage

Communication:
- stages MUST communicate via in-memory queue

QueueProperties:
- bounded buffer REQUIRED
- capacity fixed before run starts

Timing Rules:
- StartTime recorded at FetchStage entry
- EndTime recorded after RenderStage completion

Backpressure:
- queue blocks when full (no dropping allowed in Config A unless explicitly logged)

## Config B — Single-Stage Pipeline
PipelineDefinition:
- single function execution: Fetch + Process + Render in same thread or call chain

Queueing:
- NOT used

Timing Rules:
- StartTime recorded at function entry
- EndTime recorded at function exit

## Metrics (must be collected per unit)
Required per input:
- input_id
- config_id (A or B)
- start_time_ms
- end_time_ms
- latency_ms
- status (SUCCESS / FAIL)

System metrics (per run):
- throughput_items_per_sec
- total_runtime_ms
- total_processed_items
- total_failed_items

Optional (Config A only):
- queue_depth_samples (timestamped)
- queue_wait_time_ms

## Execution Procedure
For each config:
1. Initialize system
2. Load dataset into memory
3. Disable external network variability sources
4. Reset all timers and logs
5. Process dataset for 1000 repetitions per input
6. Append results only (no overwrite)
7. Terminate execution

No configuration changes allowed during run.

## Logging Format (strict)
Each line MUST be:

config_id,input_id,iteration,start_time_ms,end_time_ms,latency_ms,status

Rules:
- CSV only
- no headers in run logs
- append-only file
- no aggregation during execution

## Output Artifacts
Each run produces:
- EXP-ARCH-BASE-RUN-A.log
- EXP-ARCH-BASE-RUN-B.log

Each file is immutable after completion.

## Constraints
- No interpretation during execution
- No runtime optimization changes during run
- No modification of dataset between configs
- Identical hardware and thread allocation required for both runs
