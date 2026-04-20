# ARCH Terminology [Core Vocabulary]
ID: ARCH-TERMINOLOGY  
Status: STABLE  
Depends on: STD-DOC

## Purpose
Defines shared structural meaning of terms used across ARCH and SPEC documents.

This document defines vocabulary only.  
It does NOT define behavior, execution rules, or system policies.

## ContentBuffer
A data container representing a unit of input passed into the system.

Structure:
- payload: raw input data [any media type]
- metadata: optional contextual information

Property:
- immutable after creation

## ProcessedBuffer
A data container representing the output of processing.

Structure:
- transformed_payload
- processing_timestamp
- model_id
- processing_status

## Execution Mode
A label indicating how processing is intended to be performed.

Values:
- latency
- throughput

Note:  
This is a classification label only.  
No behavioral meaning is defined here.

## Drop
A state indicating that an input was not processed into output.

No cause or rule is defined in this document.

## Degraded
A state indicating output was produced with reduced computation fidelity.

No definition of trigger conditions is included here.

## Buffer Overflow
A state indicating input capacity was exceeded in a processing context.

No policy or resolution mechanism is defined here.

## Atomic Statement
A unit of specification representing a single indivisible concept.

Indivisible means:
- removing any part changes its meaning

No mapping rules are defined here.

## Non-Meaning Boundaries
This document does NOT define:
- execution behavior
- scheduling rules
- system architecture
- runtime policies
- hardware constraints
