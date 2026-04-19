# ARCH Layer Rules [NORMATIVE]
ID: ARCH-LAYER-RULES  
Status: PRELIMINARY  
Depends on: STD-DOC

## Purpose
Defines how architectural statements are classified as decisions, alternatives, or constraints before potential promotion to SPEC.

## Core Constraint
ARCH is non-binding design space. Nothing in ARCH defines system behavior.

Behavior is defined only in SPEC.

## Classification System
Every ARCH statement MUST be labeled exactly one of:

### FIXED
- Selected design decision
- Represents chosen intent only
- Does NOT imply completeness or promotability

### CANDIDATE
- Alternative or unselected design option
- Not selected
- Not eligible for promotion

### OBSERVED
- External fact or empirical constraint
- Not a design decision
- Not eligible for promotion

### UNRESOLVED
- Required decision or missing information
- Indicates absence of necessary structure or definition
- Must not exist in any FIXED-dependent path at time of promotion

## Atomicity Rule
All FIXED statements MUST be expressible as atomic statements.

An atomic statement is:
- a single behavior or constraint
- that cannot be decomposed further without loss of meaning

## Promotion Eligibility Rule
Only FIXED statements are eligible for promotion consideration.

A FIXED statement is promotable ONLY IF all of the following are true:
- it is expressed in atomic form
- it contains no unresolved dependencies (direct or indirect)
- each atomic statement can map to exactly one SPEC behavior

If any condition is not met, the FIXED statement is not promotable.

## Dependency Constraint Rule
If any FIXED statement depends on an UNRESOLVED element:
- promotion MUST fail for that statement path
- UNRESOLVED must be resolved or removed before promotion

## Consistency Rule
FIXED statements MUST not conflict within the same document.

If conflict exists:
- FIXED classification MUST be revised before promotion is attempted
