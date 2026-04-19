# System Map
ID: ARCH-SYS-MAP  
Status: PRELIMINARY  
Depends on: STD-DOC

## Purpose
Structural decomposition of system components for conceptual understanding only.

## Constraint
This document defines structure only.

It does NOT define:
- execution order
- runtime behavior
- guarantees
- constraints
- interfaces

All behavioral definitions belong to SPEC layer only.

## Components

### Fetcher
Retrieves web content as raw input.

### Loader
Optional external content retrieval component.

### MLProcessor
Applies ML models to input data.

### Renderer
Produces output representation from processed data.

## Data Representation
Intermediate data is represented as ContentBuffer (conceptual only).

## Important Constraint
This document does NOT define:
- execution order guarantees
- runtime behavior
- interface constraints
- security rules

Those are defined in SPEC layer only.

## Note
This is a structural diagram, not an execution specification.
