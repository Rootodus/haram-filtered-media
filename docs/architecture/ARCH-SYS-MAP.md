# System Map
ID: ARCH-SYS-MAP  
Status: PRELIMINARY  
Depends on: STD-DOC

## Conceptual Model
SystemType: streaming ML-augmented renderer.  
Goal: Filter web content using ML models with high throughput.  
Design requirements: Repeatable execution AND stability drive architectural selection.

## Pipeline Topology
The system uses a staged pipeline architecture.  
Data flows unidirectional from retrieval to presentation.  
Stages communicate via asynchronous bounded channels.  
Stages are structural boundaries ONLY.

## Interaction Surface
The system boundary allows GET requests ONLY.  
POST, PUT, AND DELETE operations are PROHIBITED at the interface layer.  
Client-side JS execution is disabled to ensure input consistency.

## Component Roles

### Fetcher
The Fetcher retrieves static web content.  
It produces the initial `ContentBuffer` for the pipeline.

### Loader
The Loader retrieves dynamic OR API-driven content.  
It MUST be isolated from core processing stages to prevent interference.

### MLProcessor
The MLProcessor transforms `ContentBuffer` payloads using ML models.  
It is stateless AND input-output consistent.  
It carries NO internal state between invocations.

### Renderer
The Renderer serializes processed content for final output.  
It decouples processing timing from presentation timing.

### Data Flow
1. Fetcher OR Loader retrieves raw data.
2. Data is encapsulated into a `ContentBuffer`.
3. `ContentBuffer` passes through the MLProcessor for transformation.
4. Renderer converts the transformed `ContentBuffer` into output format.
5. All stages prioritize shared memory references to reduce allocation overhead.

## Notes / Explanatory
- [EXPLANATORY] This document provides the mental model for system design.
- [EXPLANATORY] Normative execution constraints are defined in `SPEC` class documents.
