# Decision Logging Rules [NORMATIVE]
ID: LOG-RULES  
Status: STABLE  
Depends on: @STD-DOC, @EXP-RULES

## Evidence Requirement
- No DECISION can be recorded as STABLE without a reference to an `EXP-SPIKE` or a bullet in `ARCH-REQ`.
- Prioritize data-driven rationale over interpretive opinions if possible.

## Immutability
- This log is APPEND-ONLY.
- To change a decision, add a new entry that marks the previous ID as `[SUPERSEDED]`.
