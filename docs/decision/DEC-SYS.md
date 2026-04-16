# System Decisions
ID: DEC-SYS  
Status: PRELIMINARY  
Depends on: STD-DOC, INT-ARCH, INT-DATA-MOD, INT-PIPE

## Decision: Read-only GET mode
Constraint: GET requests ONLY  
Constraint: POST PROHIBITED  
Constraint: PUT PROHIBITED  
Constraint: DELETE PROHIBITED  
Impact: input-output equivalence under identical inputs and reduced side-effect surface increased  
Reason: side effects reduction

## Decision: Disable JS execution
Constraint: JSExecution NONE  
Constraint: dynamic content uses Loader  
Impact: execution path variance reduced under identical inputs  
Reason: DOM mutation elimination

## Decision: Pipeline architecture
Constraint: staged pipeline used  
Constraint: stages communicate via queues ONLY  
Impact: component scaling enabled  
Reason: separation of concerns

## Decision: Stateless MLProcessor
Constraint: MLProcessor has no persistent state  
Constraint: state exists in ContentBuffer only  
Impact: parallel execution improved  
Reason: batching simplification

## Decision: Async queue communication
Constraint: async channels used between stages  
Constraint: queue capacity configurable  
Impact: throughput increased  
Reason: execution decoupling

## Decision: Backpressure with frame dropping
Constraint: frames dropped under load  
Constraint: newest frames prioritized  
Impact: responsiveness preserved  
Reason: overload handling

## Decision: External Loader for dynamic content
Constraint: Loader handles dynamic content sources  
Constraint: Loader is optional  
Constraint: Loader is isolated  
Impact: core output stability preserved under isolated external variability  
Reason: external variability isolation

## Decision: Shared buffer ownership
Constraint: shared references used where possible  
Constraint: invalid access prevented via lifetime rules  
Impact: memory overhead reduced  
Reason: copy reduction

## Decision: LLM-oriented documentation
Constraint: key/value format used  
Constraint: each line is single fact  
Impact: parsing consistency improved  
Reason: machine interpretability

## Notes / Explanatory
- [EXPLANATORY] Output variance reduction is the primary driver for most architectural constraints.
- [EXPLANATORY] Frame dropping trades accuracy for responsiveness under load.
- [EXPLANATORY] Loader separation allows future extension without modifying core pipeline.
