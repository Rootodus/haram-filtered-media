# MLProcessor Core
ID: SPEC-ML-CORE  
Status: STABLE-FOR-IMPLEMENTATION  
Depends on: @ARCH-REQ, @SPEC-PARSER-DOM, @STYLE-RUST, @STD-DOC

## Purpose
- Define the normative requirements for the `MLProcessor` component that executes ONNX Runtime inference.
- Specify session lifecycle, threading, execution provider priority, input/output mapping, backpressure, and validation.
- Resolve the @ARCH-REQ::GAPS::SPEC-ML-CORE.

## Session Lifecycle - CORE-SESSION
- The system SHALL create one ONNX Runtime session per model defined in the user manifest (see @ARCH-REQ::PLUGIN-DECLARATIVE).
- Each session SHALL be created when the model is registered (e.g., on pipeline start or when a page navigation matches the model’s URL pattern).
- Sessions SHALL persist until the pipeline shuts down. No session recreation per inference call.
- Session creation SHALL validate the model’s input shape against the parser’s expected dimensions (see @CORE-VALIDATION).
- If session creation fails (e.g., invalid model, missing file), the system SHALL log an error and disable that model (inference becomes a no‑op for that URL pattern).

## Threading Model - CORE-THREADING
- Inference calls SHALL be executed on Tokio’s blocking thread pool via `tokio::task::spawn_blocking`.
- The `MLProcessor` SHALL NOT spawn dedicated OS threads per model.
- Rationale: The blocking pool provides sufficient concurrency for real‑time inference and avoids idle threads when models are inactive.

## Execution Provider Priority - CORE-EP
- The system SHALL attempt to use the highest‑priority GPU execution provider available on the target platform.
- Priority order (hardcoded):
  - Windows: DirectML → CPU
  - Linux: CUDA → CPU
  - macOS: CoreML → CPU
- If the highest‑priority GPU provider fails to initialize (e.g., missing DLLs, unsupported device), the system SHALL fall back to the CPU provider.
- The user manifest MAY override the provider via an optional `execution_provider` field (values: `"gpu"`, `"cpu"`). If specified, the system SHALL attempt that provider only; no fallback.

## Input and Output Mapping - CORE-IO
- Input tensor shape SHALL be exactly `[max_nodes, feature_dim]` where `max_nodes` and `feature_dim` are defined in the model manifest (see @SPEC-PARSER-DOM).
- Output tensor shape SHALL be `[max_nodes, num_actions]` where `num_actions` is the number of possible visual actions (e.g., blur, blackbox, etc.). The model SHALL output logits (or probabilities) for each action per node.
- The `MLProcessor` SHALL convert the output tensor into a `Vec<VisualAction>` as defined in @SPEC-ML-PROC::SCHEMA-OUTPUT-SPIKE:
  - For each node index `i` and action index `j`, if output[i][j] exceeds a threshold (e.g., 0.5), generate a `VisualAction` with `action_type = j` and `rect = node.rect`.
  - The threshold MAY be model‑specific and specified in the manifest.
- If the model outputs a different shape, session validation SHALL reject the model (see Section 7).

## Backpressure and Admission Control - CORE-BACKPRESSURE
- Before starting inference for a given `ContentBuffer`, the `MLProcessor` SHALL check an atomic flag (`skip_next`) indicating whether a newer buffer has arrived.
- If `skip_next` is true, the inference call SHALL be aborted immediately (return without running the model) and the flag reset.
- The flag is set by the pipeline when a newer `ContentBuffer` is available while the previous inference is still pending (i.e., `spawn_blocking` not yet finished).
- This mechanism ensures that the system does not waste cycles on stale frames, aligning with @ARCH-REQ::GPU-PRIORITY and @ARCH-SYS-MAP::MEM-ADMISSION.

## Input Shape Validation - CORE-VALIDATION
- At session creation, the system SHALL query the model’s input shape.
- The system SHALL compare the expected input shape (`[max_nodes, feature_dim]`) with the model’s declared shape.
- If the model expects a different shape (e.g., batch dimension), the system SHALL log an error and disable the model.
- If the model expects a dynamic shape (e.g., `-1` for batch size), the system SHALL accept it only if the product of static dimensions matches `max_nodes * feature_dim`.

## Security
- The `MLProcessor` module SHALL NOT have access to networking crates (enforced by `Cargo.toml` and code review).
- User‑supplied models are executed at the user’s own risk, as documented in @ARCH-REQ::SEC-OS-BOUNDARY.
- The system SHALL NOT load models from untrusted network sources; only local file paths specified in the manifest are allowed.

## Integration with Pipeline
- The `MLProcessor` SHALL be invoked by the `Extractor` stage via a `spawn_blocking` task, passing an `Arc<[f32]>` (the `InferenceTensor`).
- The `MLProcessor` SHALL return a `ProcessedBuffer` (list of `VisualAction`) wrapped in an `Arc` for zero‑copy handoff to the `Renderer`.
- The `MLProcessor` SHALL respect the hard‑sync ACK handshake indirectly: because it runs in a blocking task, the pipeline cannot produce a new `ContentBuffer` until the previous frame’s ACK is sent, which occurs after rendering. This preserves the stop‑and‑wait property.

## Notes / Explanatory
- [EXPLANATORY] `spawn_blocking` is chosen over dedicated threads to avoid idle resource consumption. For models that are computationally heavy and frequent, the blocking pool will automatically scale.
- [EXPLANATORY] The output threshold of 0.5 is a default; model manifests can override it. This allows models to be calibrated for different sensitivity.
- [GAP] Dynamic batch size (multiple frames at once) is not supported in this specification. It may be added in a future revision.
- [GAP] The exact mechanism for loading the model file (e.g., from the manifest’s `model_path`) is deferred to implementation; the spec assumes a valid path.
