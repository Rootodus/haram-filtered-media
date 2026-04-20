# MLProcessor Decision Model (Pure Decision Layer)
ID: EXP-ML-PROC-DEC  
Status: EXPERIMENTAL  
Depends on: SPEC-ML-PROC

## Purpose
Defines a pure function that maps an input and an immutable state snapshot to a single execution outcome.

This document defines decision logic only.  
It does NOT define time progression, queueing, batching, or execution orchestration.

## Input Contract
```
Input {
  payload
  timestamp
  model_id
  execution_mode ∈ {LATENCY, THROUGHPUT}
}
```

## State Snapshot Contract
```
StateSnapshot {
  current_time: timestamp
  latency_budget: duration
  model_cost(model_id) -> duration
  degradation_capable(model_id) -> boolean
}
```

Constraints:
- StateSnapshot is read-only
- No queue exists in this layer
- No batch state exists in this layer

## Output Contract
Exactly one outcome:
- PROCESS
- DROP
- DEGRADE

No other outputs are valid.

## Decision Function
```
decide(input, state_snapshot) -> outcome
```

This function MUST be:
- stateless
- side-effect free
- independent of call history

## Latency Mode Logic
```
deadline = input.timestamp + state_snapshot.latency_budget
cost = state_snapshot.model_cost(input.model_id)
```

Decision:

```
IF state_snapshot.current_time + cost > deadline:
    IF state_snapshot.degradation_capable(input.model_id):
        RETURN DEGRADE
    ELSE:
        RETURN DROP
ELSE:
    RETURN PROCESS
```

## Throughput Mode Logic (Decision-only interpretation)
Throughput mode does NOT evaluate batching here.

For this layer:

```
RETURN PROCESS
```

Constraint:
- Throughput scheduling is handled outside this function
- This function only evaluates per-item feasibility constraints if later expanded

(Reason: batching is not a property of a single-input decision function)

## Consistency Constraint
Valid implementation MUST ensure:
- identical Input + identical StateSnapshot → identical outcome
- no hidden state influence
- no external timing access
- no queue or batch dependency

## Non-Goals
This layer does NOT define:
- queues
- batching
- buffering
- scheduling policies
- process boundaries
- IPC or threading
- execution ordering across inputs
