# Interpretation Rules [NORMATIVE]
Project: MLFilteredBrowser (MLFB)  
ID: META-INT-RULES-0001  
Status: PRELIMINARY  
Depends on: META-SYS-OVER-0001, META-WORK-MOD-0001, DOC-STD-0001

## Purpose
Define how documentation is read, classified, and validated across all layers.

This document defines interpretation logic ONLY.  
This document does NOT define system behavior or workflow structure.

## Document Reading Principle
All documents MUST be interpreted strictly within their assigned layer.

IF a document mixes multiple layers THEN it is INVALID and MUST be split.

## Layer Classification Rules
A document belongs to exactly ONE layer.

### META classification
IF document describes:
- structure of documentation system
- interpretation rules
- workflow definitions

THEN classify as META

### INTENT classification
IF document describes:
- assumptions
- hypotheses
- unverified architecture or pipeline ideas

THEN classify as INTENT

### CONTRACT classification
IF document describes:
- executable constraints
- testable conditions
- required system behavior

THEN classify as CONTRACT

### EXPERIMENTS classification
IF document describes:
- raw outputs
- logs
- measurements
- execution traces

THEN classify as EXPERIMENTS

### DECISIONS classification
IF document describes:
- validated conclusions
- selected architectures
- rejected alternatives based on evidence

THEN classify as DECISIONS

## Ambiguity Resolution Rule
IF a document fits multiple categories THEN:
1. Identify executable constraints → CONTRACT priority
2. Identify raw outputs → EXPERIMENTS priority
3. Identify validated conclusions → DECISIONS priority
4. Otherwise classify as INTENT

META is ONLY selected if no system-specific content exists.

## Layer Contamination Detection
A document is INVALID IF:
- It contains elements from more than one layer
- It enforces behavior outside CONTRACT
- It contains interpretation inside EXPERIMENTS
- It introduces conclusions inside INTENT

## Traceability Validation
All non-META documents MUST satisfy:
- CONTRACTS MUST be traceable to INTENT
- EXPERIMENTS MUST be traceable to CONTRACTS
- DECISIONS MUST be traceable to EXPERIMENTS

Failure of traceability = INVALID document

## Interpretation Priority Rule
IF conflict exists between interpretation and content THEN:
- CONTRACT content overrides INTENT assumptions
- EXPERIMENTS override INTENT and CONTRACT assumptions
- DECISIONS override prior INTENT conclusions

META does NOT override any layer.

## Minimality Principle
IF classification is uncertain THEN:
- Choose the least authoritative valid layer
- Prefer INTENT over CONTRACT unless explicitly testable
- Prefer CONTRACT over DECISIONS unless validated experimentally

## System Stability Condition
The documentation system is stable only if:
- All documents are correctly classified
- No cross-layer mixing exists
- All transitions follow workflow model
- All interpretations are consistent across layers

## Core Constraint
Interpretation rules define how documents are read ONLY.

Interpretation rules do not define system behavior or execution.
