# MLProcessor Decision Model (Minimal Reproducible Execution Consistency State Machine)
ID: EXP-ML-PROC-DEC  
Status: EXPERIMENTAL  
Depends on: SPEC-ML-PROC

## Purpose
Defines a minimal reproducible execution consistency state machine for MLProcessor execution.

This model specifies behavior only. It does not define architecture, process boundaries, or deployment structure.

## 1. Time Model (Reproducibility Anchor)
All time-dependent decisions use `state.current_time`.

- No system clock usage is allowed
- Time must be externally supplied per evaluation step

## 2. State Definition
```
State {
  queue: list of Input
  current_time: timestamp
  latency_budget: duration
  batch_threshold: integer
  batch_interval: duration
  buffer_limit: integer
  model_cost(model_id) -> duration
  degradation_capable(model_id) -> boolean
  last_batch_time: timestamp
}
```

All fields are required for reproducible execution consistency evaluation.

## 3. Input Definition
```
Input {
  payload
  timestamp
  model_id
  execution_mode ∈ {LATENCY, THROUGHPUT}
}
```

## 4. Output Space
Each evaluation returns exactly one outcome:
- PROCESS
- DROP
- DEGRADE

QUEUE is NOT an output. It is a state mutation only.

## 5. Core Evaluation Model
Decision function:

```
decide(input, state) -> outcome + state_update
```

Evaluation order:
1. Queue update
2. Mode selection
3. Feasibility evaluation
4. Outcome resolution

No alternative ordering is valid.

## 6. Queue Update Rules

### 6.1 Latency Mode
On input arrival:

```
queue := [input]
```

All previous entries are discarded.

Queue size ≤ 1 always.

### 6.2 Throughput Mode
On input arrival:

```
if queue.size >= buffer_limit:
    queue := queue[1:]   // drop oldest

append(queue, input)
```

## 7. Latency Mode Execution

### 7.1 Deadline Computation
```
deadline = input.timestamp + latency_budget
cost = model_cost(input.model_id)
```

### 7.2 Decision Rule
```
if current_time + cost > deadline:
    if degradation_capable(input.model_id):
        return DEGRADE
    else:
        return DROP
else:
    return PROCESS
```

## 8. Throughput Mode Execution

### 8.1 Batch Trigger Condition
A batch executes when:

```
queue.size >= batch_threshold
OR
(current_time - last_batch_time) >= batch_interval
```

### 8.2 Batch Execution Semantics
```
batch = queue
queue = []
last_batch_time = current_time
```

For each input in batch (FIFO order):

```
PROCESS(input)
```

### 8.3 Batch Output Semantics
Batch execution returns:

```
list of ProcessedBuffer in FIFO order
```

Each input produces exactly one output.

## 9. Processing Rule
```
PROCESS(input):
    output = ML_MODEL(input.payload)
    return ProcessedBuffer(output)
```

## 10. DROP Semantics
DROP is terminal and irreversible.

DROP occurs only in these cases:

### Latency Mode
- deadline violation AND no degradation path exists

### Throughput Mode
- buffer overflow (oldest input removed during insertion)

DROP always removes input from system state immediately.

## 11. Degradation Rule
Only valid in latency mode:

```
if current_time + model_cost(input.model_id) > deadline
AND degradation_capable(input.model_id):
    return DEGRADE
```

Meaning:
- reduced precision OR
- smaller model OR
- partial inference

No implementation method is specified.

## 12. Reproducible Execution Consistency Constraint
A valid implementation MUST guarantee:
- identical input + identical state -> identical output
- no hidden scheduling logic
- no system-clock dependency
- full queue observability

## 13. Non-Goals
This model does NOT define:
- threading model
- process or IPC boundaries
- networking or transport layer
- hardware acceleration strategy
- ML model internals
- rendering or UI systems

## 14. Semantic Boundary
This document defines only:

```
Input + State -> Output + State Update
```

It does not define where or how execution occurs.
