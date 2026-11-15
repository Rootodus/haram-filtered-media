# FUTURE: client-server pivot (deferred)

**Status:** not doing now. Finish local reader v0.1 first. Recorded so it is not
re-litigated.

## Insight

Browser extension + localhost Rust server beats a local reader for web text.

- Extensions get DOM access free; local fetchers need per-site parsers, auth
  handling, and break on social media.
- Server holds the model once for all tabs; extension stays thin.
- hfm-player must stay native (YouTube needs yt-dlp). Text has no such
  constraint.

Not a redefinition: `hfm-reader` stays the local-file reader. The extension +
server is a separate product. Candidate name: `hfm-web-filter`.

## Python Measurement Findings (Completed)

We tested `bart-base-detox` (ONNX) in Python on ParaDetox, a realistic rant, and a strict banned-word test set. Key findings:

1. **Model Quality is High**: BLEU 0.687, Cosine 0.896 on ParaDetox. The model naturally rewrites well.
2. **Model Fails on Strict Words**: Out-of-distribution, the model fails 10-17% of the time, copying banned words (especially religious terms) directly from the input.
3. **Logit Suppression is Mandatory**: Unmasked argmax failed only 0.15% of the time, but output failure was 17%. This proves the model uses cross-attention copying and subword splits, bypassing simple argmax checks.
4. **Preemptive Masking Works**: Using `NoBadWordsLogitsProcessor`, banned rates dropped to near-zero (0.0% on ParaDetox, 1.0% on strict). Quality did not drop (Cosine 0.895).
5. **Regex Fallback is Still Needed**: Masking caught 99%, but the remaining 1% (rare casing/subword edge cases) means a regex post-filter is mathematically required to guarantee 0%.
6. **Latency is Too High**: Base latency is ~500ms/segment (CPU). Masking adds a 60% penalty in Python. This requires Int8 quantization and a native Rust Trie implementation.

## Open Question (Resolved)

Logit suppression applies only to generative rewriters. BART is generative. Masking is required to force natural synonyms. Regex is required as a safety net for the 1% masking misses.

## Design decisions (if pivot happens)

- Transport: localhost HTTP + WebSocket. Not Native Messaging.
- Bind `127.0.0.1` only.
- Per-launch auth token in a file the extension reads; required on every request.
  Non-negotiable (any website can hit localhost).
- Port: fixed or small range with `/health` check, written to file.
- GUI window *is* the server. Start/Stop, status label with port, static note
  "Closing this window will shut down the server." No close-confirm modal.
- Server is a thread in the eframe app, not a daemon.
- Extension shows "server not running" message. Primary diagnostic.
- **Filtering Strategy**: Preemptive Trie-based Logit Masking + Regex Fallback.

## Reuse

`TextSource`, `TextFilter`, `TextBuffer`, `PipelineController` survive on the
server. `LocalFileSource` becomes one source among many. eframe/egui UI does not
carry over.

## Sequencing

1. ~~Python measurement~~ (Done).
2. Quantize the ONNX model to Int8 to drop base latency from ~500ms to <200ms.
3. Rust parity: Implement Beam Search + Trie Masking + Regex Fallback in `BartDetoxFilter`. (Python `NoBadWordsLogitsProcessor` is too slow; Rust needs O(1) Trie).
4. Extension + localhost server.
