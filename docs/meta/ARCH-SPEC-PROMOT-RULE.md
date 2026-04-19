# Architecture to Spec Promotion Rule [NORMATIVE]
ID: ARCH-SPEC-PROMOT-RULE  
Status: STABLE  
Depends on: STD-DOC, DOC-MUT-POLICY

## Purpose
Defines deterministic transformation from ARCH layer (design space) into SPEC layer (contract space).

## Layer Definitions

### Architecture Layer
- Non-binding design space
- Allows contradictions and alternatives
- Used for exploration and decomposition
- MAY contain multiple FIXED candidates alongside CANDIDATE and OBSERVED entries

### Spec Layer
- Binding contract space
- Defines exact system behavior
- MUST be unambiguous and implementable
- Conflicts are prohibited within a single SPEC

## Promotion Rule
Content MAY move from architecture to specs ONLY IF all FIXED entries satisfy all transformation requirements.

Promotion is a structural rewrite operation, not a selection process.

## 1. FIXED-Only Input Requirement
Only FIXED-classified ARCH content is eligible for promotion.

- IF content is CANDIDATE -> MUST NOT be promoted
- IF content is OBSERVED -> MUST NOT be promoted
- IF no FIXED content exists -> PROMOTION FAILS

## 2. Constraint Extraction Requirement
All FIXED statements MUST be partitioned into:
- CONCRETE: directly implementable behavior
- NON_CONCRETE: removed during promotion

Only CONCRETE statements MAY be included in SPEC.

IF classification is not possible -> PROMOTION FAILS.

## 3. Deterministic Mapping Requirement
Each FIXED ARCH statement MUST map to exactly one SPEC statement.

Mapping rule:
- 1 ARCH statement -> 1 SPEC constraint OR 1 SPEC structural element

IF a statement requires many-to-one mapping -> ARCH MUST be decomposed before promotion.

## 4. Dependency Closure Requirement
All referenced concepts MUST be resolvable within:
- target SPEC file OR
- explicitly declared SPEC dependencies

IF unresolved reference exists -> PROMOTION FAILS.

## 5. Output Completeness Requirement
Resulting SPEC MUST satisfy:
- no alternative designs remain
- no unclassified statements remain
- no ARCH-only terminology remains

IF any violation exists -> PROMOTION FAILS.

## Promotion Process
When promotion occurs:
1. Extract FIXED ARCH content only
2. Remove CANDIDATE and OBSERVED content
3. Rewrite into SPEC-compliant deterministic form
4. Convert CONCRETE statements into normative SPEC constraints
5. Assign correct SPEC ownership
6. Record decision in LOG system

## Non-Promotion Rules
The following MUST NOT be promoted:
- CANDIDATE designs (unselected or alternative options)
- incomplete interface sketches
- contradictory designs
- high-level descriptions without behavior definition
- unspecified improvements or proposals

## Stability Rule
Once promoted:
- SPEC becomes authoritative source of truth
- ARCH MUST NOT be modified to align retroactively with SPEC
- ARCH divergence is allowed and treated as historical design space

## Drift Rule
If divergence occurs:
- SPEC takes precedence
- ARCH remains non-synchronizing by default
- reconciliation is optional and only for clarity improvement

## Purpose Boundary
- ARCH answers: "What possible system structures exist?"
- SPEC answers: "What exact system behavior is enforced?"
- Promotion is the only transformation bridge between them
