# ARCH Layer Rules [NORMATIVE]
ID: ARCH-LAYER-RULES  
Status: PRELIMINARY  
Depends on: STD-DOC

## Status Interpretation Rule
This document defines non-authoritative design space behavior.

- Requirements here are NOT binding.
- Components described here are NOT finalized.
- Any constraint expressed here MUST be redefined in SPEC layer before implementation.
- If conflict exists between sections, no resolution is assumed.
- Any statement in ARCH that defines `MUST/SHALL/PROHIBITED` behavior is non-binding and must be rewritten during SPEC promotion.

## Decision Status Rule
Every ARCH statement MUST explicitly classify itself as one of:
- CANDIDATE: exploratory design option, not eligible for SPEC promotion
- FIXED: selected design intent eligible for SPEC promotion
- OBSERVED: factual constraint from experiments or external systems

## State Semantics

### CANDIDATE
- Represents alternative or incomplete design options
- MUST NOT be treated as system structure
- MUST NOT be promoted to SPEC

### FIXED
- Represents selected architectural intent
- MAY be promoted to SPEC if all promotion constraints are satisfied
- MUST be internally consistent within the ARCH document

### OBSERVED
- Represents external constraint or empirical fact
- MUST NOT be interpreted as design choice
- MAY inform FIXED selection but is not promotable itself

## Promotion Eligibility Rule
Only FIXED items are eligible for SPEC promotion.

CANDIDATE and OBSERVED items are explicitly excluded from promotion.
