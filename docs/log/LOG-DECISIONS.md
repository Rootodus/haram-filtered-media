# Decision Log
ID: LOG-DECISIONS  
Status: STABLE  
Depends on: STD-DOC, EXP-ARCH-BASELINE

## Decision: Read-only GET mode
Statement: The system boundary allows GET requests ONLY.  
Evidence: `EXP-ARCH-BASELINE`.

Observed signals:
- Lower variance in request-side state in Configuration A.
- Reduced side-effect surface in constrained interaction model.

Relationship mapping:
- Interaction surface restriction aligns with fewer observed state variation paths.
- Reduced mutation surface aligns with reduced output variability under identical inputs.

Rejected alternatives:
- Allowing mixed HTTP methods with runtime filtering (higher observed divergence).

## Decision: Disable JS execution
Statement: JS execution is disabled in the content loading environment.  
Evidence: `EXP-ARCH-BASELINE`.

Observed signals:
- Client-side execution introduces variability in rendered input representation.
- DOM mutation introduces inconsistent preprocessing states.

Relationship mapping:
- Removal of client-side execution aligns with more stable input extraction patterns.

Rejected alternatives:
- Sandboxed JS execution with partial allowlist (introduces timing variance).

## Decision: Pipeline architecture
Statement: System uses a staged pipeline architecture.  
Evidence: `EXP-ARCH-BASELINE`.

Observed signals:
- Configuration A shows higher scheduling overhead.
- Configuration B shows lower overhead but reduced isolation between stages.

Relationship mapping:
- Queue-based separation aligns with higher overhead AND stronger stage decoupling.
- Single-stage execution aligns with lower overhead AND increased coupling.

Rejected alternatives:
- Fully single-stage execution (loss of separation boundaries).

## Decision: Stateless MLProcessor
Statement: `MLProcessor` MUST NOT maintain persistent state between invocations.  
Evidence: `EXP-ARCH-BASELINE`.

Observed signals:
- Stateful retention correlates with higher run-to-run variance.
- Stateless execution correlates with more consistent outputs.

Relationship mapping:
- Absence of cross-run state aligns with reduced variability.

Rejected alternatives:
- Session-based state model (higher cross-run coupling observed).

## Decision: Async communication
Statement: Stages communicate via asynchronous bounded channels.  
Evidence: `EXP-ARCH-BASELINE`.

Observed signals:
- Synchronous coupling increases upstream idle time.
- Queue-based buffering smooths throughput under burst conditions.

Relationship mapping:
- Decoupling via queues aligns with reduced blocking propagation.

Rejected alternatives:
- Direct synchronous chaining (backpressure amplification observed).

## Decision: Buffer sharing
Statement: Buffers use shared references where safe.  
Evidence: `EXP-ARCH-BASELINE`.

Observed signals:
- Copy-based handling increases memory usage with payload size.
- Shared references reduce allocation overhead in measured runs.

Relationship mapping:
- Reduced copying aligns with lower memory usage in observed runs.

Rejected alternatives:
- Deep copy per stage boundary (higher allocation cost observed).

## Decision: Documentation format
Statement: Standardize on atomic key/value formatting AND STE logic.  
Evidence: `ARCH-SYSTEM-MAP` structural evaluation.

Observed signals:
- Free-form blocks increase parsing ambiguity for AI models.
- Atomic key/value formatting reduces interpretation variance.

Relationship mapping:
- Structural constraints align with more consistent parsing behavior.

Rejected alternatives:
- Free-form documentation blocks (higher ambiguity observed).

## Notes / Explanatory
- [EXPLANATORY] This log is append-only to preserve the historical rationale.
- [EXPLANATORY] All decisions MUST reference evidence from the `EXP` class.
