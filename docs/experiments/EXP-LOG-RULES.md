# Experiment Logging Rules [NORMATIVE]
ID: EXP-LOG-RULES  
Status: STABLE  
Depends on: STD-DOC

## Purpose
Defines how experiment execution results MUST be recorded.

## Scope
Applies to all files in `experiments/`.

## Core Rule
Experiment logs MUST represent observed execution only.

They MUST NOT contain:
- design intent
- architectural decisions
- specification definitions
- interpretive conclusions

## Required Structure
Each experiment run MUST include:
- input_trace: raw inputs exactly as received
- output_trace: exact system outputs
- metrics: numerical aggregates of execution
- runtime: execution configuration used
- determinism_check: replay consistency result

## Trace Integrity Rule
input_trace and output_trace MUST NOT be modified or inferred.

They represent factual execution history.

## Metrics Rule
Metrics MUST be derived only from traces.

No external estimation is allowed.

## Observation Rule
Observations are optional human notes.

They MUST NOT affect metrics or determinism results.

## Reproducibility Rule
Every experiment MUST be replayable from:
- input_trace
- runtime configuration

If replay fails -> experiment is invalid.
