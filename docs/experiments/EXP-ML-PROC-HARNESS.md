# MLProcessor Experiment Harness (Execution Neutral Test Framework)
ID: EXP-ML-PROC-HARNESS  
Status: EXPERIMENTAL  
Depends on: EXP-ML-PROC-DEC, EXP-ML-PROC-SCHED

## Purpose
Defines a fixed experimental environment for executing and comparing MLProcessor implementations.

This document does NOT define architecture, decision logic, or scheduling logic.

It defines only:
- input generation
- execution interface contract
- state initialization rules
- logging and observability rules

## Core Principle
All implementations under test MUST be evaluated under identical conditions:
- same input stream
- same timing progression
- same initial state
- same observation format

Any deviation invalidates comparison results.

## Input Stream Generator

### Definition
The experiment uses a fixed event sequence:

```
EventStream {
  inputs: list of Input
  time_model: monotonic timestamp progression
}
```

### Constraints
- Input order MUST be fixed before execution begins
- No runtime input mutation is allowed
- No adaptive input generation is permitted

### Time Progression Rule
```
state.current_time MUST be externally advanced per step
```

Valid models:
- fixed delta progression
- explicit timestamp list

Invalid models:
- system clock usage
- wall-clock dependency
- event-driven implicit timing

## Execution Interface Contract
All implementations MUST expose the same interface:

```
step(input: Input, state: State) -> StepResult
```

### StepResult Definition
```
StepResult {
  outcome: PROCESS | DROP | DEGRADE
  outputs: list of ProcessedBuffer
  state_update: State
}
```

### Constraint
- One step = one input evaluation
- No hidden batch execution inside step
- No background processing threads affecting output

## State Initialization Rules

### Required Consistency Rule
All systems MUST initialize from identical state seeds:

```
InitialState {
  queue: empty
  current_time: 0
  last_batch_time: 0
  latency_budget: fixed
  batch_threshold: fixed
  batch_interval: fixed
  buffer_limit: fixed
  model_cost table: fixed
  degradation_capability table: fixed
}
```

### Constraint
- No preloaded queue state allowed
- No warm-up phase allowed
- No hidden initialization logic permitted

## Unified Logging Schema
Every implementation MUST emit identical log structure:

```
LogEntry {
  step_index: integer
  input: Input
  observed_state: StateSnapshot
  decision_outcome: outcome
  execution_result: optional ProcessedBuffer list
  queue_state: optional snapshot
}
```

### Logging Rules
- Logs MUST be append-only
- Logs MUST NOT influence execution
- Logs MUST capture state BEFORE and AFTER decision

### Required Observations
Each step MUST log:
- input received
- decision outcome
- resulting queue state
- execution output (if any)

## Execution Modes Under Test
The harness does NOT interpret logic differences.

It only supplies identical conditions for:
- EXP-ML-PROC-DEC (decision-centric model)
- EXP-ML-PROC-SCHED (scheduler-centric model)

Each system is treated as a black box implementing `step()`.

## Execution Procedure

### Run Loop
```
FOR each input IN EventStream:
    result = step(input, state)
    log(result)
    state = result.state_update
```

### Constraint
- No parallel execution allowed
- No speculative execution allowed
- No reordering of inputs allowed

## Comparison Rule
After execution completes:

Comparison is performed ONLY on logs:
- outcome sequence
- output sequence
- state transitions

No other signals are valid.

## Validity Constraint
An experiment is INVALID if:
- input streams differ
- state initialization differs
- timing progression differs
- logging format differs
- execution order differs

## Non-Goals
This harness does NOT define:
- scheduling policy
- decision logic
- ML model behavior
- threading or concurrency model
- distributed execution
- hardware acceleration
- implementation language

## Structural Principle
This document defines the experimental boundary:

```
fixed environment + fixed inputs
        ↓
   interchangeable implementations
        ↓
   comparable execution traces
```

Without this layer, architecture comparison is undefined.
