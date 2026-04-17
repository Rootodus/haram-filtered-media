<!-- Full Path: /contract/CONTR-ARCH.md -->

# Architecture Contracts
ID: CONTR-ARCH  
Status: PRELIMINARY  
Depends on: STD-DOC, INT-ARCH

## Scope Boundary
- This contract defines system-wide architectural constraints
- It MUST NOT override execution semantics defined in CONTR-EXEC-BASE
- It MUST NOT define benchmark measurement rules

## Read-only request mode (external interface constraint only)
Constraint:
- Only GET requests are allowed at system boundary
- POST, PUT, DELETE are prohibited at interface layer

Source:
- INT-ARCH

## Disable JS execution (external content constraint only)
Constraint:
- JS execution MUST be disabled in content loading environment
- Dynamic content MUST be handled via Loader subsystem only

Source:
- INT-ARCH

## Pipeline architecture (structural constraint only)
Constraint:
- System uses a staged pipeline architecture
- Stages are structural concepts only (see INT-PIPE)
- Execution semantics are defined exclusively in CONTR-EXEC-BASE

Source:
- INT-PIPE

## Stateless MLProcessor
Constraint:
- MLProcessor MUST NOT maintain persistent state between invocations
- All state MUST be carried in ContentBuffer or execution context

Source:
- INT-DATA-MOD

## Queue communication model
Constraint:
- Stages communicate via asynchronous bounded channels (conceptual)
- Capacity and behavior MUST be defined in CONTRACT execution layer

Source:
- INT-PIPE

## External Loader subsystem
Constraint:
- Loader handles dynamic or API-driven content retrieval
- Loader MUST be isolated from core processing stages
- Loader is optional at system composition level

Source:
- INT-PIPE

## Buffer ownership
Constraint:
- Buffers SHOULD prefer shared ownership where safe
- Lifetime safety MUST be enforced by implementation layer

Source:
- INT-DATA-MOD

## Documentation format constraint
Constraint:
- Each line MUST represent a single atomic constraint
- Key/value structure MUST be preserved

Source:
- INT-ARCH
