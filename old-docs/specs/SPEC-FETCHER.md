# Fetcher Specification
ID: SPEC-FETCHER  
Status: STABLE  
Depends on: STD-DOC, SPEC-CONTENT-BUFFER, ARCH-SYSTEM-MAP

## Goal
The Fetcher retrieves external OR static web content AND encapsulates it into the initial `ContentBuffer` for pipeline ingestion.

## Interface
- `FetchStage(input: PipelineMessage) -> PipelineMessage`

## SIGNAL Handling Constraint
Constraint:
- ALL stages MUST implement a pattern match for `PipelineMessage`.
- `DATA` variants MUST be processed according to the specific stage logic.
- `SIGNAL` variants MUST bypass stage logic AND be returned as the output.

## Constraints

### Functional Scope
Constraint:
- Fetcher MUST perform input retrieval OR pass-through logic ONLY.
- Fetcher MUST NOT perform ML transformation.
- Fetcher MUST NOT perform rendering logic.

Rationale:
- Strict isolation of retrieval prevents side-effect leakage into downstream transformation stages.

### Output Requirements
Constraint:
- Fetcher MUST return a valid `ContentBuffer` regardless of retrieval outcome.
- IF retrieval fails, THEN the Fetcher MUST set a `FAIL` status flag within the `ContentBuffer` metadata.

Rationale:
- Pipeline continuity requires a valid buffer object to ensure stable termination OR error logging in the Renderer stage.

### JS Execution
Constraint:
- Fetcher MUST NOT execute JavaScript during the retrieval process.

Rationale:
- Aligns with `SPEC-SYSTEM-RULES` to ensure input consistency AND prevent unstable state DOM mutations before ML processing.

## Dependency Constraint
Constraint:
- This component is strictly coupled to the Universal Interface: `SPEC-CONTENT-BUFFER`.
- Generated code MUST NOT assume fields OR metadata keys NOT defined in `SPEC-CONTENT-BUFFER`.

## Refined Failure Behavior
Constraint:
- On retrieval failure:
  1. Set `metadata.status` to `FAIL`.
  2. Set `metadata.error_code` to the relevant HTTP or System code.
  3. Set `metadata.error_msg` to a descriptive string.
  4. Set `payload` to an EMPTY byte array.
  5. Set `content_type` to `Text` (default fallback).

## Notes / Explanatory
- [EXPLANATORY] The Fetcher is the primary entry point for the pipeline.
- [EXPLANATORY] Simpler retrieval mechanisms improve input consistency for the MLProcessor.
