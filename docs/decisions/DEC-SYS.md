# System Decisions
ID: DEC-SYS  
Status: PRELIMINARY  
Depends on: DOC-STD, INT-ARCH, INT-DATA-MOD, INT-PIPE

## Decision: Read-only GET mode
Design: system uses GET requests ONLY  
Reason: avoid side effects and unpredictable server interaction  
Impact: deterministic input for MLProcessor  
Constraint: POST, PUT, DELETE are PROHIBITED

## Decision: Disable JS execution
Design: JSExecution is NONE  
Reason: JS introduces non-deterministic DOM mutation  
Impact: stable and predictable pipeline input  
Constraint: dynamic content MUST use Loader

## Decision: Pipeline architecture
Design: system uses staged pipeline  
Reason: separation of concerns and parallel execution  
Impact: independent scaling of Fetcher, MLProcessor, Renderer  
Constraint: stages communicate via queues ONLY

## Decision: Stateless MLProcessor
Design: MLProcessor has no persistent state  
Reason: simplifies parallelism and batching  
Impact: predictable performance and easier scaling  
Constraint: all required state MUST exist in `ContentBuffer`

## Decision: Async queue communication
Design: stages communicate via async channels  
Reason: decouple execution and enable concurrency  
Impact: improved throughput and fault isolation  
Constraint: queue capacity MUST be configurable

## Decision: Backpressure with frame dropping
Design: system drops frames under load  
Reason: maintain real-time responsiveness  
Impact: possible data loss in high load conditions  
Constraint: dropping MUST prioritize newest frames

## Decision: External Loader for dynamic content
Design: Loader handles JS-required or API-driven content  
Reason: isolate non-deterministic sources  
Impact: core pipeline remains deterministic  
Constraint: Loader is OPTIONAL and isolated

## Decision: Shared buffer ownership
Design: buffers use shared references where possible  
Reason: reduce memory copying overhead  
Impact: improved performance for large media data  
Constraint: lifetime management MUST prevent invalid access

## Decision: LLM-oriented documentation
Design: documentation uses key/value and STE structure  
Reason: improve machine parsing and code generation  
Impact: consistent interpretation by LLMs  
Constraint: each line MUST represent a single fact

## Notes / Explanatory
- [EXPLANATORY] Determinism is the primary driver for most architectural constraints.
- [EXPLANATORY] Frame dropping trades accuracy for responsiveness under load.
- [EXPLANATORY] Loader separation allows future extension without modifying core pipeline.
