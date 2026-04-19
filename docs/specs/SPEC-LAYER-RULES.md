# SPEC Layer Rules [NORMATIVE]
ID: SPEC-LAYER-RULES  
Status: STABLE  
Depends on: STD-DOC

## Purpose
Defines constraints for SPEC documents as the authoritative system behavior layer.

## Core Property
Each SPEC defines exactly one responsibility domain.

A SPEC is invalid if it mixes multiple independent responsibilities.

## Non-Redefinition Rule
A SPEC MUST NOT redefine external concepts.

It MAY reference external concepts but MUST NOT modify their meaning or structure.

## Dependency Rule
SPEC dependencies MUST form a Directed Acyclic Graph (DAG).

Cycles are not allowed.

If a cycle exists, SPEC boundaries must be split.

## Shared Concept Rule
Each shared concept MUST have exactly one owning SPEC.

Ownership includes:
- defining structure
- defining lifecycle rules
- defining mutation rules

All other SPECs may only reference the concept.

## Overlap Rule
If two SPECs describe overlapping responsibility:
- the overlap MUST be extracted into a new SPEC
- original SPECs MUST be reduced in scope

Duplication is not allowed as a resolution strategy.

## Drift Rule
If a SPEC expands beyond its original responsibility:
- it MUST be split before further modification
