# Loader Specification
ID: SPEC-LOADER  
Status: STABLE  
Depends on: STD-DOC, SPEC-CONTENT-BUFFER, SPEC-SYSTEM-RULES

## Goal
The Loader retrieves dynamic OR API-driven content that requires specialized handling outside the static `Fetcher` scope. It encapsulates retrieved data into a `ContentBuffer`.

## Interface
`LoadStage(input: URL) -> ContentBuffer`

## Constraints

### Functional Isolation
Constraint:
- Loader MUST operate as a standalone subsystem isolated from core `MLProcessor` logic.
- Loader MUST NOT share internal state with the `Fetcher`.

Rationale:
- Isolation prevents unstable dynamic retrieval overhead from blocking the primary static ingestion path.

### Execution Policy
Constraint:
- Loader MUST handle content requiring client-side simulation OR API authentication.
- Loader MUST NOT propagate JS execution state into the `ContentBuffer`.
- Loader MUST return a valid `ContentBuffer` object even on retrieval failure.

Rationale:
- Offloading dynamic complexity to the Loader preserves the execution consistency of the downstream pipeline.

### Error Mapping
Constraint:
- IF an API OR dynamic source is unreachable, THEN the Loader MUST populate the `Metadata` with `status: FAIL` AND `error_code` from the source.

## Refined Failure Behavior
Constraint:
- On retrieval failure:
  1. Set `metadata.status` to `FAIL`.
  2. Set `metadata.error_code` to the relevant HTTP or System code.
  3. Set `metadata.error_msg` to a descriptive string.
  4. Set `payload` to an EMPTY byte array.
  5. Set `content_type` to `Text` (default fallback).

## Notes / Explanatory
- [EXPLANATORY] The Loader is optional at system composition level.
- [EXPLANATORY] Use the Loader ONLY when the `Fetcher` cannot retrieve content via standard HTTP GET.
