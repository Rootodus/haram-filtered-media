# System Decisions
ID: DEC-SYS  
Status: PRELIMINARY  
Depends on: STD-DOC, INT-ARCH, INT-DATA-MOD

## Decision Schema (normative)
Each decision MUST contain:
- Decision statement (descriptive, non-normative)
- Evidence references (≥1 EXP artifact)
- Observed signals (derived from EXP data only)
- Relationship mapping (signal -> observed association in data)
- Rejected alternatives (when comparison exists)

## Decision: Read-only GET mode
Evidence:
- EXP-ARCH-BASE-RUN-A.log
- EXP-ARCH-BASE-RUN-B.log

Observed Signals:
- Lower variance in request-side state in Config A
- Reduced side-effect surface in constrained interaction model

Relationship Mapping:
- Restriction of interaction surface aligns with fewer observed state variation paths
- Reduced mutation surface aligns with reduced output variability under identical inputs

Rejected Alternatives:
- Allowing mixed HTTP methods with runtime filtering (higher observed divergence)

## Decision: Disable JS execution
Evidence:
- EXP-ARCH-BASE-RUN-A.log

Observed Signals:
- Client-side execution introduces variability in rendered input representation
- DOM mutation introduces inconsistent preprocessing states

Relationship Mapping:
- Removal of client-side execution aligns with more stable input extraction patterns

Rejected Alternatives:
- Sandboxed JS execution with partial allowlist (still introduces timing variance)

## Decision: Pipeline architecture
Evidence:
- EXP-ARCH-BASE-RUN-A.log
- EXP-ARCH-BASE-RUN-B.log

Observed Signals:
- Config A shows higher scheduling overhead
- Config B shows lower overhead but reduced isolation between stages

Relationship Mapping:
- Queue-based separation aligns with higher overhead and stronger stage decoupling
- Single-stage execution aligns with lower overhead and increased coupling

Rejected Alternatives:
- Fully single-stage execution (loss of separation boundaries)

## Decision: Stateless MLProcessor
Evidence:
- EXP-ARCH-BASE-RUN-A.log

Observed Signals:
- Stateful retention correlates with higher run-to-run variance
- Stateless execution correlates with more consistent outputs

Relationship Mapping:
- Absence of cross-run state aligns with reduced variability

Rejected Alternatives:
- Session-based state model (higher cross-run coupling observed)

## Decision: Async communication
Evidence:
- EXP-ARCH-BASE-RUN-A.log

Observed Signals:
- Synchronous coupling increases upstream idle time
- Queue-based buffering smooths throughput under burst conditions

Relationship Mapping:
- Decoupling via queues aligns with reduced blocking propagation

Rejected Alternatives:
- Direct synchronous chaining (backpressure amplification observed)

## Decision: Buffer sharing
Evidence:
- EXP-ARCH-BASE-RUN-B.log

Observed Signals:
- Copy-based handling increases memory usage with payload size
- Shared references reduce allocation overhead in measured runs

Relationship Mapping:
- Reduced copying aligns with lower memory usage in observed runs

Rejected Alternatives:
- Deep copy per stage boundary (higher allocation cost observed)

## Decision: Documentation format
Evidence:
- INT-ARCH / INT-DATA-MOD structural evaluation

Observed Signals:
- Free-form blocks increase parsing ambiguity
- Atomic key/value formatting reduces interpretation variance

Relationship Mapping:
- Structural constraints align with more consistent parsing behavior

Rejected Alternatives:
- Free-form documentation blocks (higher ambiguity observed)
