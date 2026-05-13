# Decision Logging Rules [NORMATIVE]
ID: LOG-RULES  
Status: STABLE  
Depends on: STD-DOC, EXP-RULES

## Evidence Requirement
- No DECISION can be recorded as STABLE without a reference to an `EXP-SPIKE` or a `FACT`.
- Interpretive opinions are PROHIBITED; only data-driven rationale is valid.

## Immutability
- This log is APPEND-ONLY.
- To change a decision, add a new entry that marks the previous ID as `[SUPERSEDED]`.
