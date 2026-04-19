# Architecture → Spec Promotion Rule
ID: ARCH-SPEC-PROMOTION-RULE  
Status: STABLE  
Depends on: STD-DOC, DOC-MUTATION-POLICY

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
Content MAY move from architecture to specs ONLY IF all conditions are met:

### 1. Selection
A single design option MUST be chosen from competing architectural variants.

### 2. Disambiguation
All ambiguity MUST be removed:
- no alternative paths
- no optional behaviors unless explicitly modeled as rules
- no unresolved terminology

### 3. Testability
The proposed spec MUST be expressible as:
- deterministic rules OR
- explicit conditional logic

### 4. Scope fit
The content MUST map to exactly one spec file responsibility.  
If it spans multiple responsibilities, it MUST be split before promotion.

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
