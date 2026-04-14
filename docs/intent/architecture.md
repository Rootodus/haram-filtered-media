# Architecture
ID: ARC-0001  
Status: PRELIMINARY  
Depends on: DOC-STD-0001

## System Overview
SystemType: streaming ML-augmented renderer  
Goal: filter web content using ML with high throughput  
Mode: read-only browser  
RequestType: GET only  
JSExecution: PROHIBITED  
DynamicContent: handled by Loader  
Pipeline: Fetcher -> MLProcessor -> Renderer

## Components
Component: Fetcher  
Role: fetch static web content  
Input: URL  
Output: `ContentBuffer`  
Protocol: HTTP GET  
Async: true  
Optional: false  
Notes:
- Fetcher does NOT execute JS
- Fetcher produces deterministic input for pipeline

Component: Loader  
Role: fetch dynamic content from APIs or JS-required sources  
Input: endpoint or site identifier  
Output: `ContentBuffer`  
Async: true  
Optional: true  
Notes:
- Loader isolates non-deterministic sources
- Loader MAY run in separate process

Component: MLProcessor  
Role: transform content using ML models  
Input: `ContentBuffer`  
Output: `ContentBuffer`  
Execution: multi-threaded  
Acceleration: GPU optional  
State: stateless  
Optional: false  
Notes:
- MLProcessor MUST NOT fetch data
- MLProcessor applies transformations:
  - image/video: blur animate beings
  - audio: modify pitch
  - text: filter profanity

Component: Renderer  
Role: display processed content  
Input: `ContentBuffer`  
Output: screen or audio device  
Async: true  
Optional: false  
Notes:
- Renderer MUST NOT modify content
- Renderer consumes pipeline output only

## Data Flow
PrimaryFlow: Fetcher -> MLProcessor -> Renderer  
OptionalFlow: Loader -> MLProcessor -> Renderer  
QueueModel: async channels  
QueueBound: configurable  
Backpressure: REQUIRED

## Constraints
JSExecution: NONE  
StateMutation: restricted to MLProcessor output  
SideEffects: PROHIBITED in Fetcher and MLProcessor  
Determinism: REQUIRED for pipeline input

## Notes / Explanatory
- [EXPLANATORY] Deterministic input ensures stable ML throughput and predictable latency.
- [EXPLANATORY] Loader separation prevents dynamic content from breaking pipeline guarantees.
