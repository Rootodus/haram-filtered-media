# System Driver Specification
ID: SPEC-MAIN  
Status: STABLE  
Depends on: STD-DOC, SPEC-PIPELINE, SPEC-BENCHMARK-RULES

## Goal
The System Driver (`main.rs`) manages the application lifecycle, CLI argument parsing, environment initialization, AND pipeline orchestration.

## Execution Requirements

### CLI Interface
The driver MUST support the following arguments:
- `--config`: Enum [A, B] (Determines pipeline topology as per `SPEC-PIPELINE`).
- `--input`: Path (Location of the pre-generated dataset).
- `--threads`: Integer (Size of the global thread pool for Config A).
- `--iterations`: Integer (Number of repetitions per input item).

### Initialization Sequence
1. Parse CLI arguments.
2. Load the immutable dataset into memory.
3. IF Config A, THEN initialize bounded asynchronous channels AND thread pool.
4. IF Config B, THEN initialize the synchronous call chain.
5. Initialize the `Renderer` logging sink.

### Lifecycle Management
Constraint:
- Upon `SIGINT` OR completion of the input dataset, the Driver MUST command the `Fetcher` to inject a `PipelineMessage::SIGNAL(SHUTDOWN)`.
- The Driver MUST wait for the `Renderer` thread to join before terminating the process, ensuring the "Drain" operation is complete.

### Logging Sink
Constraint:
- Driver MUST capture output from the `Renderer` AND write to the artifact specified in `SPEC-BENCHMARK-RULES`.
- Log entries MUST be flushed to disk immediately to prevent data loss on crash.

## Error Handling
Constraint:
- Driver MUST catch unhandled exceptions from stages.
- Driver MUST NOT allow a single `UnitOfWork` failure to terminate the entire process.

## Instruction Invalidation
- `SPEC-CONTENT-BUFFER` is the Universal Interface for the entire system.
- ANY modification to fields, types, OR reserved metadata keys in `SPEC-CONTENT-BUFFER` MUST be treated as a Breaking Change for ALL dependent stages (`Fetcher`, `MLProcessor`, `Renderer`, `Loader`).
- IF `SPEC-CONTENT-BUFFER` is modified, THEN ALL stage-specific source code (`src/*.rs`) MUST be re-prompted using the updated specification.
- Manual patching of AI-generated code to match new buffer fields is PROHIBITED to prevent "Hidden Drift."

## Workflow Logic: Sync-Generation

### Buffer-Driven Propagation
- The `SPEC-CONTENT-BUFFER` is the "Clock Signal" for code generation.
- Sequence for Schema Changes:
  1. Update `SPEC-CONTENT-BUFFER` with new fields OR keys.
  2. Record the rationale in `LOG-DECISIONS`.
  3. Re-feed the updated `SPEC-CONTENT-BUFFER` + the target `SPEC-STAGE` to the AI.
  4. Replace the existing `src/*.rs` with the new AI output.

### Consistency Verification
- IF a stage implementation fails to compile OR process a `ContentBuffer` field, THEN the error is a "Sync Failure."
- Sync Failures MUST be resolved by verifying that the AI had the LATEST version of the Universal Interface, NOT by manual code editing.

## Dataset Schema

### JSON Structure
The pre-generated dataset MUST follow this schema:

```json
{
  "dataset_metadata": {
    "total_items": 1000,
    "generation_seed": 42
  },
  "items": [
    {
      "input_id": "string (UUID)",
      "payload_type": "enum (Text, ImageStub)",
      "payload_size": "integer (bytes, range: 1024-1048576)",
      "expected_status": "SUCCESS"
    }
  ]
}
```

### Constraints
- `payload_size` MUST be used by the generator to create a dummy byte array of the exact size.
- `payload_type` MUST dictate the `content_type` field in the resulting `ContentBuffer`.

## Notes / Explanatory
- [EXPLANATORY] `main.rs` acts as the "Glue" between the structural specs AND the execution hardware.
- [EXPLANATORY] Threading logic in Config A MUST adhere to the bounded queue constraints defined in `SPEC-PIPELINE`.
