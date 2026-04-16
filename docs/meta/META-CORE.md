# Meta System [NORMATIVE]
Project: MLFilteredBrowser (MLFB)  
ID: META-CORE  
Status: PRELIMINARY  
Depends on: STD-DOC

## Purpose
Define documentation schema, classification rules, and lifecycle constraints.

This document defines documentation governance rules only.

## Documentation Layer Schema
The documentation system uses five layers:

### META
- Defines documentation structure and governance rules
- Defines classification rules for documents
- Defines lifecycle rules for documents
- Applies only to the documentation system itself
- MUST NOT define runtime system behavior

### INTENT
- Unverified assumptions
- No enforcement role

### CONTRACT
- Testable constraints
- Defines required system behavior

### EXPERIMENT
- Execution outputs only
- No interpretation allowed

### DECISION
- Validated conclusions from EXPERIMENT

## Layer Isolation Rules
Each document MUST belong to exactly one layer.

Layers are semantically independent.

Mixed-layer documents are INVALID.

## Classification Rules
A document MUST be assigned using rule order with fully defined evaluation precedence and a single unambiguous resolution path.

IF document contains executable constraints THEN classify as CONTRACT  
ELSE IF document contains raw outputs, logs, or measurements THEN classify as EXPERIMENT  
ELSE IF document contains validated conclusions THEN classify as DECISION  
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
- EXPERIMENT records post-execution outputs only
- DECISION is derived summaries of EXPERIMENT only
- INTENT contains unvalidated assumptions only

## Non-Authority Rule
No layer MAY modify or override another layer.

Cross-layer semantic control is prohibited.

- EXPERIMENT MUST NOT affect CONTRACT semantics
- DECISION MUST NOT affect CONTRACT semantics
- INTENT MUST NOT invalidate CONTRACT or EXPERIMENT

### Invariance Rule
- CONTRACT is immutable after definition
- EXPERIMENT is immutable after recording
- DECISION is immutable after creation

META does not participate in interpretation.

## Workflow Model
Information moves through four lifecycle stages:

### INTENT
- Unverified information
- No enforcement value

### CONTRACT
- Testable executable constraints
- Defines required behavior conditions

### EXPERIMENT
- Recorded execution outputs only
- No interpretation or evaluation applied

### DECISION
- Validated conclusions derived strictly from EXPERIMENT

### Valid Flow
INTENT -> CONTRACT -> EXPERIMENT -> DECISION

### Invalid Flows
- INTENT -> DECISION is allowed ONLY for explicit design commitments
- Such DECISION MUST NOT claim experimental validation
- CONTRACT -> INTENT
- EXPERIMENT -> CONTRACT
- DECISION -> INTENT

### Transition Rules
IF information becomes testable THEN it MAY move INTENT -> CONTRACT  
IF CONTRACT is executed THEN outputs MUST be stored in EXPERIMENT  
IF EXPERIMENT exists THEN they remain raw records only

## Traceability Requirements
- Each EXPERIMENT MUST reference at least one CONTRACT
- Each DECISION MUST reference at least one EXPERIMENT
- Each CONTRACT SHOULD reference originating INTENT
- DECISION MUST NOT be derived without referenced EXPERIMENT evidence

Failure of traceability INVALIDATES the document.

## Stability Conditions
System is stable ONLY IF:
- Each document belongs to exactly one layer
- All transitions follow workflow rules
- No cross-layer mixing exists
- Classification rules are reproducible under identical inputs and rule ordering
- No layer has semantic authority over another layer

## System Representation Constraint
The documentation system is a static schema.

It defines categorization and lifecycle rules ONLY.  
It does NOT represent runtime architecture or execution behavior.

## Core Constraint
META defines governance rules for documentation only.

META has no authority over runtime systems.
