# Architecture Governance Rules [NORMATIVE]
ID: ARCH-RULES  
Status: STABLE  
Depends on: STD-DOC

## FDG Model Enforcement
Each architecture document MUST categorize information into exactly three blocks:
1. FACTS: External constraints or empirical observations. (The "Why")
2. DECISIONS: Committed implementation instructions. (The "What")
3. GAPS: Identified technical unknowns blocking a decision. (The "To-Do")

## Commitment Rule
- `[IDEA]` and `[CANDIDATE]` tags are PROHIBITED in the DECISIONS block.
- They MUST be relegated to the NOTES section to prevent implementation ambiguity.

## Performance Alignment
Every DECISION must explicitly support Native Performance or Simple Implementation.
