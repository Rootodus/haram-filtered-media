# Requirements
ID: ARCH-REQ  
Status: STABLE  
Depends on: STD-DOC

## FACTS [Hard Constraints & Observations]
- ENV-EXT-SLOW: ML execution in browser extensions introduces overhead due to IPC serialization AND main-thread contention.
- NET-GET-ONLY: Modern network interactions allow complex mutations, but the system environment is restricted to static retrieval.
- DYN-WEB-JS: Modern websites REQUIRE JavaScript for functional content resolution.
- COMP-TRADE: High-fidelity document rendering AND high-speed ML inference are computationally competitive goals.

## DECISIONS [Committed Architecture]
- HOST-NATIVE: The system SHALL run as a standalone native process [NOT an extension] to minimize environment latency.
- SCOPE-RESTRICTED: The system IS a restricted runtime; it IS NOT a full web browser.
- NET-RESTRICT: Network interaction IS limited to `HTTP` `GET`. `POST`, `PUT`, AND `DELETE` are PROHIBITED.
- DYN-SNAPSHOT: Dynamic content MUST be resolved into a static snapshot via an external `Loader` [Headless Chrome].
- PIPE-MONOLITH: The system SHALL use a single-process monolithic pipeline to eliminate internal IPC overhead.
- MODE-SUPPORT: The system SHALL support two `ExecutionMode` values: `latency` AND `throughput`.
- USER-CONFIG: Users SHALL configure `ExecutionMode` preferences per ML model.
- SEC-OS-ISOLATION: The primary security boundary IS the host OS process isolation.
- UNIT-CONTENTBUFFER: The unit of processing IS a `ContentBuffer` containing a serialized DOM snapshot AND CSS computed styles.

## GAPS [Active Blockers]
- MAPPING: The specific algorithm for mapping DOM nodes AND CSS styles to fixed-width tensor indices IS NOT defined.
- SANDBOX: The necessity of additional sandboxing [e.g., WASM] beyond OS process isolation IS NOT defined.
- THRESHOLD: The maximum acceptable latency deviation compared to a raw `ONNX` baseline IS NOT defined.

## NOTES / EXPLANATORY
- [IDEA] `latency` mode could implement aggressive dropping of pending `ContentBuffer` items to preserve real-time constraints.
- [IDEA] `throughput` mode could implement batching of `ContentBuffer` items to maximize GPU utilization.
- [EXPLANATORY] `HOST-NATIVE` is the primary response to the observed slowness of browser extensions documented in `ENV-EXT-SLOW`.
