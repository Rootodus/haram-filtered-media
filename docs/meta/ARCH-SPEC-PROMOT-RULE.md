# Architecture to Spec Promotion Rule [NORMATIVE]
ID: ARCH-SPEC-PROMOT-RULE  
Status: STABLE  
Depends on: STD-DOC, DOC-MUT-POLICY

## Purpose
Defines how content moves from `architecture/` (design space) into `specs/` (contract space).

## Layer Definitions

### Architecture Layer
- Non-binding design space
- Allows contradictions and alternatives
- Used for exploration and decomposition
- MAY contain multiple competing models

### Spec Layer
- Binding contract space
- Defines exact system behavior
- MUST be unambiguous and implementable
- Conflicts are prohibited within a single spec

## Promotion Rule
Content MAY move from architecture to specs ONLY IF it satisfies ALL transformation requirements below.

Promotion is NOT a decision process.  
Promotion is a structural rewrite operation.

### 1. Single-Variant Requirement
Input MUST contain exactly ONE selected design variant.

- IF multiple variants exist → PROMOTION FAILS
- IF no explicit selection marker exists → PROMOTION FAILS

Selection MUST be explicitly marked as:
- `SELECTED: true`

All other variants MUST be marked:
- `SELECTED: false`

### 2. Constraint Extraction Requirement
All behavioral statements MUST be partitioned into:
- `CONCRETE` (implementable)
- `NON_CONCRETE` (removed during promotion)

Only CONCRETE statements are allowed into SPEC.

IF classification is not possible → PROMOTION FAILS.

### 3. Deterministic Rewrite Requirement
Each ARCH statement MUST map to exactly ONE SPEC statement.

Mapping rule:
- 1 ARCH statement → 1 SPEC constraint OR 1 SPEC structural element

IF many-to-one mapping is required → ARCH must be split first.

### 4. Dependency Closure Requirement
All referenced concepts MUST be resolvable inside:
- the target SPEC file OR
- imported SPEC dependencies

IF unresolved reference exists → PROMOTION FAILS.

### 5. Output Completeness Requirement
The resulting SPEC MUST satisfy:
- no alternatives remain
- no unclassified statements remain
- no ARCH-only terminology remains

IF any residual ARCH constructs remain → PROMOTION FAILS.

## Promotion Process
When promotion occurs:
1. Copy relevant architecture content
2. Rewrite into spec format (normative language only)
3. Remove all non-binding alternatives
4. Add explicit constraints and failure behavior
5. Assign correct SPEC file ownership
6. Record decision in `LOG-DECISIONS.md`

## Non-Promotion Rules
The following CANNOT be promoted:
- exploratory comparisons without a selected option
- incomplete interface sketches
- contradictory designs
- high-level descriptions without behavior definition
- “possible improvements” without selection

## Stability Rule
Once promoted:
- Spec becomes authoritative
- Architecture must not be updated to retroactively match spec
- Architecture may remain outdated without correction

## Drift Rule
If architecture diverges from spec:
- spec takes precedence
- architecture is considered historical design residue
- no synchronization is required unless explicitly useful

## Purpose Boundary
- Architecture answers: "What could the system be?"
- Spec answers: "What must the system do?"
- Promotion is the only bridge between them
