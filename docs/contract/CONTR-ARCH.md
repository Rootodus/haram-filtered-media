# Architecture Contracts
ID: CONTR-ARCH  
Status: PRELIMINARY  
Depends on: STD-DOC, INT-ARCH

## Read-only GET mode
Constraint:
- Only GET requests are allowed
- POST, PUT, DELETE are PROHIBITED

Source:
- INT-ARCH

## Disable JS execution
Constraint:
- JS execution MUST be NONE
- Dynamic content MUST use Loader

Source:
- INT-ARCH

## Pipeline architecture
Constraint:
- System MUST use staged pipeline
- Stages MUST communicate via queues ONLY

Source:
- INT-ARCH

## Stateless MLProcessor
Constraint:
- MLProcessor MUST NOT have persistent state
- All required state MUST exist in ContentBuffer

Source:
- INT-ARCH

## Async queue communication
Constraint:
- Stages MUST communicate via async channels
- Queue capacity MUST be configurable

Source:
- INT-ARCH

## Backpressure with frame dropping
Constraint:
- System MUST drop frames under load
- Dropping MUST prioritize newest frames

Source:
- INT-ARCH

## External Loader for dynamic content
Constraint:
- Loader MUST handle JS-required or API-driven content
- Loader MUST be isolated from core pipeline
- Loader MUST be optional

Source:
- INT-ARCH

## Shared buffer ownership
Constraint:
- Buffers SHOULD use shared references where possible
- Lifetime management MUST prevent invalid access

Source:
- INT-ARCH

## LLM-oriented documentation
Constraint:
- Documentation MUST use key/value structure
- Each line MUST represent a single fact

Source:
- INT-ARCH
