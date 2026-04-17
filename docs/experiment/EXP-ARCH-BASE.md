# Architecture Baseline Experiment
ID: EXP-ARCH-BASE  
Status: RAW  
Depends on: CONTR-ARCH, INT-ARCH, CONTR-EXEC-BASE, CONTR-BENCH

## Purpose
Compare two execution configurations under identical inputs and shared execution contracts.

## Unit Definition
UnitOfWork:
- single input processed from ingestion to completion

Execution semantics:
- MUST follow CONTR-EXEC-BASE

## Dataset
Type: synthetic deterministic set

Structure:
- input_id: string
- payload_type: enum(Text, ImageStub)
- payload_size: integer

Rules:
- dataset MUST be identical across Config A and Config B
- dataset MUST be pre-generated before execution begins
- dataset MUST remain immutable during execution (as defined in CONTR-BENCH)

Repetitions:
- each input MUST be processed 1000 times per configuration

## Config A — Staged Pipeline
Stages:
1. FetchStage
2. ProcessStage (MLProcessor)
3. RenderStage

Communication:
- bounded queue between stages (defined in CONTR-EXEC-BASE)

## Config B — Single-Stage Pipeline
Definition:
- Fetch + Process + Render executed in a single call chain

## Outputs
Artifacts:
- EXP-ARCH-BASE-RUN-A.log
- EXP-ARCH-BASE-RUN-B.log

## Notes
- Execution behavior, timing rules, logging format, and hardware constraints are defined in CONTR-EXEC-BASE and CONTR-BENCH.
- This document does not redefine execution semantics.
