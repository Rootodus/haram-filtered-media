# System Overview [NORMATIVE]
Project: MLFilteredBrowser (MLFB)  
ID: META-SYS-OVER-0001  
Status: PRELIMINARY  
Depends on: DOC-STD-0001

## Purpose
Define the documentation layer ontology.

This document defines ONLY structural categories.  
This document MUST NOT define classification logic, workflow transitions, or interpretation rules.

## Document Layer System
The system defines five layers.

### META
- Defines documentation structure boundaries
- Defines system-level organization of documents
- Has no behavioral, classification, or transition logic authority

### INTENT
- Contains unverified assumptions
- Contains hypotheses and speculative design states
- Has no enforcement or validation authority

### CONTRACT
- Defines executable constraints
- Defines testable system requirements
- Defines required conditions for system behavior

### EXPERIMENTS
- Stores raw execution outputs
- Stores logs and measurement data
- Contains no interpretation or validation logic

### DECISIONS
- Stores validated conclusions derived from EXPERIMENTS
- Represents accepted system knowledge state
- Requires traceability to EXPERIMENTS

## Layer Independence Rule
Each document MUST belong to exactly ONE layer.

Each layer MUST remain semantically isolated.

- INTENT MUST NOT contain executable constraints
- CONTRACT MUST NOT contain assumptions or hypotheses
- EXPERIMENTS MUST NOT contain interpretation or conclusions
- DECISIONS MUST NOT introduce unvalidated assumptions
- META MUST NOT define classification rules or workflow logic

## System Representation Constraint
The documentation system is a static layered ontology.

It represents information categorization ONLY.  
It does NOT represent execution behavior or runtime architecture.

## Cross-Layer Separation Rule
IF information spans multiple layers THEN it MUST be split into separate documents.

Mixed-layer documents are INVALID.

## Stability Principle
The ontology is stable ONLY IF:
- Each document maps to exactly one layer
- No overlapping responsibilities exist between layers
- No layer defines or modifies another layer

## Non-Goals
- Classification rules are NOT defined here
- Workflow transitions are NOT defined here
- System behavior is NOT defined here
- Runtime architecture is NOT defined here

## Core Constraint
META defines structure ONLY.

META has no classification authority, no workflow authority, and no execution authority.
