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
- Inference calls for each model SHALL be executed on Tokio’s blocking thread pool via `tokio::task::spawn_blocking`.
- The `MLProcessor` SHALL spawn one `spawn_blocking` task per active model for a given `ContentBuffer`.
- Multiple models SHALL run in parallel on the blocking pool, provided sufficient CPU cores are available.
- Each model maintains its own `ort::Session` instance; sessions are not shared across threads or tasks.
- The render thread SHALL wait for all spawned inference tasks to complete before presenting the frame (using `futures::join_all` or `tokio::try_join!`).
- The blocking thread pool SHALL be sized to the number of CPU cores, enabling true parallel execution of multiple models.
- Rationale: Parallel execution avoids sequential bottlenecks while reusing the blocking pool for efficiency.

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
- When multiple models are active for the same `ContentBuffer`, the `MLProcessor` SHALL collect the `Vec<VisualAction>` from each model and concatenate them into a single `ProcessedBuffer` (order unspecified).
- If two models produce actions on the same node and same action type, both are kept; the renderer applies them sequentially.
- If the model outputs a different shape, session validation SHALL reject the model (see @CORE-VALIDATION).

## Backpressure and Admission Control - CORE-BACKPRESSURE
- The system SHALL maintain a single atomic flag (`skip_next`) indicating whether a newer `ContentBuffer` has arrived while any inference task from the previous frame is still pending.
- If `skip_next` is true, the `MLProcessor` SHALL abort all pending inference tasks for the new frame (i.e., not spawn them) and reset the flag.
- The flag is set by the pipeline when a newer `ContentBuffer` is available and at least one previous inference task is still running (i.e., any `spawn_blocking` not yet finished).
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
- The `MLProcessor` SHALL be invoked by the `Extractor` stage for each model defined in the manifest.
- For each model, the `MLProcessor` SHALL spawn a `spawn_blocking` task that receives the `Arc<[f32]>` (the `InferenceTensor`) and returns a `Vec<VisualAction>`.
- After all tasks complete, the `MLProcessor` SHALL merge the results into a single `ProcessedBuffer` (list of `VisualAction`), wrapped in an `Arc` for zero‑copy handoff to the `Renderer`.
- The `MLProcessor` SHALL respect the hard‑sync ACK handshake indirectly: the render thread waits for all inference tasks to finish before presenting, so the ACK is sent only after all models have executed for that frame.

## Notes / Explanatory
- [EXPLANATORY] `spawn_blocking` is chosen over dedicated threads to avoid idle resource consumption. For models that are computationally heavy and frequent, the blocking pool will automatically scale.
- [EXPLANATORY] The output threshold of 0.5 is a default; model manifests can override it. This allows models to be calibrated for different sensitivity.
- [EXPLANATORY] Parallel model execution is implemented via multiple `spawn_blocking` tasks. This does not violate the `Send`/`Sync` requirements because each task uses its own `ort::Session` (sessions are not shared).
- [GAP] Dynamic batch size (multiple frames at once) is not supported in this specification. It may be added in a future revision.
- [GAP] The exact mechanism for loading the model file (e.g., from the manifest’s `model_path`) is deferred to implementation; the spec assumes a valid path.
