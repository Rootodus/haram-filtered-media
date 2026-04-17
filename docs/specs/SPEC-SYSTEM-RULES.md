# System Rules
ID: SPEC-SYSTEM-RULES  
Status: STABLE  
Depends on: STD-DOC, ARCH-SYSTEM-MAP

## Interface Constraints

### Request Method Restriction
Constraint:
- The system MUST allow GET requests ONLY at the external boundary.
- POST, PUT, AND DELETE operations are PROHIBITED.

Rationale:
- Restricting request types reduces side-effect surface area AND prevents unpredictable external state changes.
- High-fidelity content extraction requires a stable, read-only interaction model.

### Execution Environment
Constraint:
- JS execution MUST be disabled within the content loading environment.
- Dynamic content requirements MUST be offloaded to the Loader subsystem.

Rationale:
- Disabling JS prevents unstable state DOM mutations AND reduces variability in rendered input representation.
- Isolation of dynamic retrieval ensures the core pipeline remains focused on static state transformation.

## Component Constraints

### MLProcessor Statelessness
Constraint:
- MLProcessor MUST NOT maintain persistent state between invocations.
- ALL required state MUST be encapsulated within the `ContentBuffer` OR the immediate execution context.

Rationale:
- Statelessness ensures reproducibility of ML transformations AND simplifies parallel scaling across shared-nothing clusters.

### Loader Isolation
Constraint:
- The Loader subsystem MUST be isolated from core processing stages.
- Core pipeline stages MUST NOT depend on the internal state of the Loader.

Rationale:
- Decoupling retrieval mechanisms prevents unstable external behavior from propagating into the ML transformation layer.

## Structural Constraints

### Pipeline Topology
Constraint:
- The system MUST adhere to a staged pipeline architecture.
- Communication between stages MUST use asynchronous bounded channels.

Rationale:
- Staged separation enables independent scaling of Fetcher, MLProcessor, AND Renderer components.
- Bounded channels prevent memory exhaustion during ingestion bursts.

## Notes / Explanatory
- [EXPLANATORY] These rules represent the "Hard Logic" the AI MUST follow when generating system components.
- [EXPLANATORY] Any violation of these rules in generated code is a failure of the specification.
