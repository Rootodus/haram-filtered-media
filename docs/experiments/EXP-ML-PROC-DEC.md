# MLProcessor Decision Model (Minimal Deterministic State Machine)
ID: EXP-ML-PROC-DEC  
Status: EXPERIMENTAL  
Depends on: SPEC-ML-PROC

## Purpose
Defines a minimal deterministic decision function for MLProcessor execution.

This model is implementation-oriented, but does not prescribe architecture, threading, or process boundaries.

## 1. State Definition
```
State {
  queue: list of Input
  current_time: timestamp
  latency_budget: duration
  batch_threshold: integer
  batch_interval: duration
  buffer_limit: integer
  model_cost(model_id) → duration
  degradation_capable(model_id) → boolean
  last_batch_time: timestamp
}
```

All fields are required for deterministic evaluation.  
`last_batch_time` is required to make batch triggering deterministic.

## 2. Input Definition
```
Input {
  payload
  timestamp
  model_id
  execution_mode ∈ {LATENCY, THROUGHPUT}
}
```

## 3. Output Types
Exactly one per input:
- PROCESS
- DROP
- DEGRADE
- QUEUE (only meaningful before scheduling decision is resolved)

Final system outcome MUST resolve to one of:
- PROCESS
- DROP
- DEGRADE

QUEUE is an intermediate state, not a final output.

## 4. Core Rule
Decision is a single deterministic function:

```
decide(input, state) → outcome
```

Evaluation order is fixed:
1. Mode selection
2. Queue update
3. Feasibility check
4. Execution decision

No rule priority system exists outside this order.

## 5. Latency Mode (Single-Slot Policy)

### 5.1 Queue Rule
On input arrival:

```
queue := [input]
```

All previous entries are discarded immediately.

Queue size is always exactly 0 or 1.

### 5.2 Decision Rule
```
estimated_cost = model_cost(input.model_id)
deadline = input.timestamp + latency_budget
```

```
IF current_time + estimated_cost > deadline:
    IF degradation_capable(input.model_id):
        RETURN DEGRADE
    ELSE:
        RETURN DROP
ELSE:
    RETURN PROCESS
```

## 6. Throughput Mode (Batch Policy)

### 6.1 Queue Rule
```
IF queue.size >= buffer_limit:
    DROP oldest(queue)

append(input)
```

### 6.2 Batch Trigger Rule
Batch execution occurs when:

```
queue.size >= batch_threshold
OR (current_time - last_batch_time) >= batch_interval
```

### 6.3 Batch Execution Rule
```
batch = queue (FIFO order)
queue := empty
last_batch_time := current_time
```

For each input in batch:

```
process(input)
```

Each input produces exactly one outcome.

## 7. Processing Rule
```
process(input):
    output = ML_MODEL(input.payload)
    return PROCESS(output)
```

## 8. Degradation Rule
Only applies in latency mode.

```
IF current_time + model_cost(input.model_id) > deadline
AND degradation_capable(input.model_id):
    RETURN DEGRADE
```

Degradation means reduced computation, such as:
- smaller model variant
- reduced precision inference
- partial computation path

No specific technique is mandated.

## 9. DROP Rules (Global)
DROP is terminal and overrides all other outcomes.

```
DROP if:
- latency deadline violated AND no degradation available
- buffer_limit exceeded in throughput mode (oldest entry)
```

## 10. Determinism Constraint
Valid implementation MUST ensure:
- identical input + identical state → identical output
- no hidden scheduling behavior outside this model
- queue state is fully observable and serializable

## 11. Non-Goals
This model does NOT define:
- threading model
- process separation (single vs multi-process)
- IPC, sockets, or network transport
- hardware acceleration strategy
- ML model internals
- rendering or UI system

## 12. Interpretation Rule
This document defines only:

```
Input + State → Deterministic Outcome
```

It does not define how the function is deployed or executed.
