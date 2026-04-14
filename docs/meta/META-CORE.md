# Meta System [NORMATIVE]
Project: MLFilteredBrowser (MLFB)  
ID: META-CORE  
Status: PRELIMINARY  
Depends on: DOC-STD

## Purpose
Define documentation ontology, classification rules, and lifecycle constraints.

This document defines governance rules ONLY.  
This document MUST NOT define runtime system behavior.

## Documentation Layer Ontology
The system defines five layers:

### META
- Defines documentation structure and governance rules
- Defines classification rules for documents
- Defines lifecycle rules for documents
- Applies only to the documentation system itself
- MUST NOT define runtime system behavior

### INTENT
- Contains unverified assumptions and hypotheses
- Represents speculative design state
- Has no enforcement authority

### CONTRACT
- Defines executable constraints
- Defines testable requirements
- Defines required system behavior conditions

### EXPERIMENTS
- Stores raw execution outputs only
- Stores measurements, logs, and trace data only
- MUST NOT contain interpretation, validation, conclusions, or inference

### DECISIONS
- Stores validated conclusions derived strictly from EXPERIMENTS
- Represents accepted system knowledge state
- Requires traceability to EXPERIMENTS

## Layer Isolation Rules
Each document MUST belong to exactly ONE layer.

Each layer MUST remain semantically isolated.

- INTENT MUST NOT contain executable constraints
- CONTRACT MUST NOT contain assumptions or hypotheses
- EXPERIMENTS MUST NOT contain interpretation or conclusions
- DECISIONS MUST NOT introduce unvalidated assumptions
- META MUST NOT define runtime system behavior

Mixed-layer documents are INVALID.

## Classification Rules
A document MUST be assigned deterministically using rule order.

IF document contains executable constraints THEN classify as CONTRACT  
ELSE IF document contains raw outputs, logs, or measurements THEN classify as EXPERIMENTS  
ELSE IF document contains validated conclusions THEN classify as DECISIONS  
ELSE IF document contains assumptions or hypotheses THEN classify as INTENT  
ELSE IF document contains structural or governance rules about this documentation system THEN classify as META  
ELSE document is INVALID

## Ambiguity Rule
IF multiple classification rules apply THEN document MUST be split.

No priority-based selection is permitted.

## Interpretation Rules
Documents MUST be interpreted only within their assigned layer.

Cross-layer interpretation is INVALID.

### Strict Boundary Rule
- CONTRACT defines pre-execution constraints
- EXPERIMENTS record post-execution outputs only
- DECISIONS are derived summaries of EXPERIMENTS only
- INTENT contains unvalidated assumptions only

### Non-Authority Rule
No layer can modify, override, or redefine another layer.

Specifically:
- EXPERIMENTS MUST NOT affect CONTRACT semantics
- DECISIONS MUST NOT affect CONTRACT semantics
- INTENT cannot invalidate CONTRACT or EXPERIMENTS

### Invariance Rule
- CONTRACT is immutable after definition
- EXPERIMENTS are immutable after recording
- DECISIONS are immutable after creation

META does not participate in interpretation.

## Workflow Model
Information moves through four lifecycle stages:

### INTENT
- Unverified information
- No enforcement value

### CONTRACT
- Testable executable constraints
- Defines required behavior conditions

### EXPERIMENTS
- Recorded execution outputs only
- No interpretation or evaluation applied

### DECISIONS
- Validated conclusions derived strictly from EXPERIMENTS

### Valid Flow
INTENT -> CONTRACT -> EXPERIMENTS -> DECISIONS

### Invalid Flows
- INTENT -> DECISIONS (without EXPERIMENTS)
- CONTRACT -> INTENT
- EXPERIMENTS -> CONTRACT
- DECISIONS -> INTENT

### Transition Rules
IF information becomes testable THEN it MAY move INTENT -> CONTRACT  
IF CONTRACT is executed THEN outputs MUST be stored in EXPERIMENTS  
IF EXPERIMENTS exist THEN they remain raw records only

## Traceability Requirements
- Each EXPERIMENT MUST reference at least one CONTRACT
- Each DECISION MUST reference at least one EXPERIMENT
- Each CONTRACT SHOULD reference originating INTENT

Failure of traceability INVALIDATES the document.

## Stability Conditions
System is stable ONLY IF:
- Each document belongs to exactly one layer
- All transitions follow workflow rules
- No cross-layer mixing exists
- Classification rules are deterministic and non-ambiguous
- No layer has semantic authority over another layer

## System Representation Constraint
The documentation system is a static ontology.

It defines categorization and lifecycle rules ONLY.  
It does NOT represent runtime architecture or execution behavior.

## Core Constraint
META defines governance rules for documentation only.

META has no authority over runtime systems.
