# SPEC Layer Rules [NORMATIVE]
ID: SPEC-LAYER-RULES  
Status: STABLE  
Depends on: STD-DOC

## Single Responsibility Closure
Each SPEC file MUST define exactly ONE responsibility domain.

A responsibility domain is defined as:
- a single transformation stage OR
- a single system capability OR
- a single interface contract

If a SPEC contains multiple domains -> it is INVALID.

## No Cross-SPEC Redefinition
A SPEC MUST NOT redefine:
- data structures defined in other SPEC files
- behavior defined in other SPEC files
- lifecycle rules owned by other SPEC files

It MAY reference them, but NOT redefine them.

## Dependency Direction Rule
SPEC dependencies MUST form a DAG:
- cycles are PROHIBITED
- reverse dependency inference is PROHIBITED

If a cycle appears -> SPEC decomposition is REQUIRED.

## Shared Concept Ownership Rule
Every shared concept (type, buffer, message, protocol) MUST have EXACTLY ONE owner SPEC.

Ownership means:
- defining structure
- defining lifecycle rules
- defining mutation rules

All other SPECs MAY use the concept but MUST NOT redefine it.

## Overlap Detection Rule
If two SPECs describe overlapping responsibility:
- overlap MUST be extracted into a new SPEC
- original SPECs MUST be reduced to non-overlapping roles

No “shared responsibility by duplication” is allowed.

## Drift Rule
If a SPEC evolves such that:
- its responsibility scope expands OR
- it begins redefining external concepts

Then:
- it MUST be split before further modification
