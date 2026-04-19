# ARCH Layer Rules [NORMATIVE]
ID: ARCH-LAYER-RULES  
Status: PRELIMINARY  
Depends on: STD-DOC

## Status Interpretation Rule
This document is non-authoritative design space.

- Requirements here are NOT binding.
- Components described here are NOT finalized.
- Any constraint expressed here MUST be redefined in SPEC layer before implementation.
- If conflict exists between sections, no resolution is assumed.
- Any statement in ARCH that defines `MUST/SHALL/PROHIBITED` behavior is non-binding and must be rewritten during SPEC promotion.

## Decision Status Rule
Every ARCH document MUST explicitly classify statements as one of:
- UNDECIDED: candidate design, not selected
- DECIDED: intended final system shape, but not yet specified in SPEC
- OBSERVED: factual constraint from experiments or external systems

Only DECIDED items may be promoted to SPEC.  
UNDECIDED items MUST NOT be treated as system structure.
