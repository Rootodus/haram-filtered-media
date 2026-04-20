# MLProcessor Scheduler (Execution Layer)
ID: EXP-ML-PROC-SCHED  
Status: EXPERIMENTAL  
Depends on: EXP-ML-PROC-DEC

## Purpose
Defines how inputs are ordered, queued, batched, and fed into the decision function.

## State
```
SchedulerState {
  queue: list of Input
  last_batch_time: timestamp
  buffer_limit: integer
  batch_threshold: integer
  batch_interval: duration
}
```

## Queue Rules

### Latency Mode
On input arrival:

```
queue := [input]
```

All previous inputs are discarded.

### Throughput Mode
On input arrival:

```
IF queue.size >= buffer_limit:
    queue := queue[1:]   // drop oldest

append(queue, input)
```

## Scheduling Loop
The scheduler runs in discrete steps driven by external time progression.

At each step:

### Step A — Check Batch Trigger
```
IF queue.size >= batch_threshold
OR (current_time - last_batch_time) >= batch_interval:
    trigger_batch = true
ELSE:
    trigger_batch = false
```

### Step B — Batch Execution
If `trigger_batch == true`:

```
batch = queue
queue = []
last_batch_time = current_time
```

### Step C — Evaluation
For each input in batch:

```
outcome = decide(input, snapshot)
execute(outcome, input)
```

## Execution Semantics

### PROCESS
- ML model is executed
- produces output buffer

### DROP
- input is discarded
- no execution occurs

### DEGRADE
- reduced-cost model execution is used
- produces output buffer

## Model Execution Boundary
```
execute(outcome, input):
    IF outcome == PROCESS:
        run full model
    IF outcome == DEGRADE:
        run reduced model
    IF outcome == DROP:
        no-op
```

## Key Separation Rule
- DEC layer: answers “what should happen to this input”
- SCHED layer: answers “when and in what order inputs are evaluated”

No logic overlap is allowed.

## Consistency Constraint
Scheduler MUST ensure:
- same input stream + same initial state + same time progression

→ identical sequence of outcomes

## Non-Goals
This layer does NOT define:
- ML model internals
- hardware acceleration
- network transport
- threading model implementation details (only sequencing semantics)
- distributed execution

## Structural Principle
System correctness depends on strict separation:

```
Scheduling (time + queue + batching)
        ↓
Decision (pure mapping)
        ↓
Execution (model invocation)
```

Mixing these layers invalidates reproducibility of behavior across implementations.
