# Meta Core [NORMATIVE]
Project: MLFilteredBrowser (MLFB)  
ID: META-CORE  
Status: PRELIMINARY  
Depends on: STD-DOC

## Scope Boundary
- This document defines only documentation governance, classification, and dependency rules
- This document MUST NOT define runtime system behavior
- This document MUST NOT define execution semantics
- This document MUST NOT define benchmark semantics

All behavioral exclusions are defined in canonical META rules only.

## Document Universe Model
The system is a directed acyclic graph (DAG) of documents.

### Nodes
- Each document is a node

### Edges
- Each dependency reference is a directed edge
- Edge direction: dependency -> dependent

## Layer Model
Each document MUST belong to exactly one layer:
- META
- INTENT
- CONTRACT
- EXPERIMENT
- DECISION

Layer assignment is exclusive and non-overlapping.

## Layer Definitions

### META
Defines canonical governance rules for:
- classification
- dependency validity
- lifecycle constraints

MUST NOT define runtime behavior of any external system.

### INTENT
- Unvalidated assumptions
- Hypotheses only
- No executable constraints allowed

### CONTRACT
- Testable system constraints
- Required behavior specifications
- May be referenced by EXPERIMENT

### EXPERIMENT
- Structured execution record schema
- Contains datasets, logs, metrics, and outputs
- MAY include reproducibility metadata
- MUST NOT contain interpretive conclusions

### DECISION
- Conclusions derived strictly from EXPERIMENT outputs
- Must reference at least one EXPERIMENT

## Rule System Model (Canonical)
All constraints in META-CORE are atomic and uniquely defined.

### Rule Uniqueness
- Each rule MUST have exactly one canonical definition
- Repetition of identical semantic rules is INVALID

### Derived Statements
- Any repeated statement is DERIVED
- Derived statements MUST NOT introduce new constraints

### Conflict Rule
If two rules overlap semantically:
- they are treated as the same rule
- they MUST be merged or one marked as derived

## Classification Rules (Deterministic)
1. CONTRACT: testable constraints
2. EXPERIMENT: raw execution records
3. DECISION: validated conclusions
4. INTENT: assumptions
5. META: governance rules
6. else INVALID

If multiple apply:
- document MUST be split

## Dependency Graph Rules

### Allowed Direction
INTENT -> CONTRACT -> EXPERIMENT -> DECISION

### Allowed Edges
- INTENT -> CONTRACT
- CONTRACT -> CONTRACT
- CONTRACT -> EXPERIMENT
- EXPERIMENT -> EXPERIMENT
- EXPERIMENT -> DECISION
- DECISION -> DECISION

### Forbidden Edges
- CONTRACT -> INTENT
- EXPERIMENT -> CONTRACT
- DECISION -> CONTRACT
- DECISION -> EXPERIMENT
- INTENT -> EXPERIMENT
- INTENT -> DECISION

### Acyclic Constraint
- Graph MUST be acyclic

## Traceability Requirements
- CONTRACT SHOULD reference INTENT
- EXPERIMENT MUST reference CONTRACT
- DECISION MUST reference EXPERIMENT

Failure invalidates document.

## Invariance Rules
- Documents are immutable after creation
- Changes require new node

## Graph Consistency Rules
- No dangling references
- All dependencies must resolve
- Deterministic resolution required

## Stability Conditions
System is stable if:
- single-layer assignment holds
- DAG is acyclic
- traceability rules are satisfied

## EXPERIMENT Schema Definition
EXPERIMENT is a structured execution record, not a process.

### Required Fields
- dataset reference
- metric schema
- execution outputs
- reproducibility metadata

### Constraint
- EXPERIMENT does NOT define execution semantics

## LINKED EXPERIMENT
Definition:
- references external CONTRACT

Requirements:
- MUST reference CONTRACT identifier(s)
- MUST follow external BENCH rules
- MUST be reproducible from referenced CONTRACT

Properties:
- participates in DAG validation
- supports comparability

Constraint:
- does NOT embed CONTRACT logic

## SELF-CONTAINED EXPERIMENT (EXPERIMENT-SC)
Definition:
- fully closed execution record with embedded reproducibility snapshot

Requirements:
- MUST embed resolved CONTRACT snapshot
- MUST embed metric schema snapshot
- MUST embed dataset definition snapshot
- MUST be internally consistent at creation time

Constraints:
- MUST NOT reference external CONTRACTs for execution
- MUST NOT claim equivalence to external CONTRACT versions

## Metric Compatibility Rule
Two EXPERIMENTS are comparable iff:
- metric schema fields match exactly (name, type, aggregation)

If not:
- they are non-comparable
- neither is invalidated

## Execution Interpretation Rule
- EXPERIMENT does not define execution behavior
- It defines execution records only
- Execution itself is external to META system

## Rule Precedence
1. META-CORE
2. CONTRACT
3. EXPERIMENT
4. INTENT
5. DECISION

Higher overrides lower if conflict exists.

## Core Constraint
META defines documentation governance only.  
META does not participate in runtime execution or system behavior.
