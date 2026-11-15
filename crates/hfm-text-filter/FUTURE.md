# hfm-text-filter: Architecture & Implementation

**Status:** Pivot accepted. The local-file reader scope was deemed less useful than a web filter. `hfm-reader` is being renamed and morphed into `hfm-text-filter` (client-server architecture).

## Insight

Browser extension + localhost Rust server beats a local reader for web text.

- Extensions get DOM access free; local fetchers need per-site parsers, auth
  handling, and break on social media.
- Server holds the model once for all tabs; extension stays thin.
- hfm-player must stay native (YouTube needs yt-dlp). Text has no such
  constraint.

The crate is renamed to `hfm-text-filter`. The extension is a separate client project.

## Python Measurement Findings (Completed)

We tested `bart-base-detox` (ONNX) in Python on ParaDetox, a realistic rant, and a strict banned-word test set. Key findings:

1. **Model Quality is High**: BLEU 0.687, Cosine 0.896 on ParaDetox. The model naturally rewrites well.
2. **Model Fails on Strict Words**: Out-of-distribution, the model fails 10-17% of the time, copying banned words (especially religious terms) directly from the input.
3. **Logit Suppression is Mandatory**: Unmasked argmax failed only 0.15% of the time, but output failure was 17%. This proves the model uses cross-attention copying and subword splits, bypassing simple argmax checks.
4. **Preemptive Masking Works**: Using `NoBadWordsLogitsProcessor`, banned rates dropped to 0.0% across all test sets. Quality did not drop (Cosine 0.895).
5. **Regex Fallback is Still Needed**: Masking caught 100% in our tests, but to guarantee against adversarial casing or tokenizer edge cases in the wild, a regex post-filter remains a mandatory safety net.
6. **INT8 Quantization is a Trap**: Dynamic INT8 quantization degraded model quality (BLEU dropped from 0.68 to 0.63) and did *not* improve latency due to CPU decompression overhead. We must stick to FP32.
7. **Greedy Search (Beam=1) is Optimal**: Dropping from Beam=5 to Beam=1 cut latency in half (~790ms to ~446ms) with *zero* loss in BLEU or Cosine. The model does not need beam search to find good detoxifications.
8. **Python is Too Slow**: Even with Greedy search, Python overhead limits us to ~446ms/segment. The final server must be written in Rust (`ort` crate) to eliminate GIL and PyTorch overhead.

## Design Decisions

- **Name**: `hfm-text-filter`.
- **Transport**: localhost HTTP + WebSocket (Axum).
- **Bind**: `127.0.0.1` only.
- **Auth**: Per-launch auth token in a file the extension reads; required on every request. (Non-negotiable; any website can hit localhost).
- **Port**: Fixed or small range with `/health` check, written to file.
- **GUI**: The eframe window *is* the server control panel. Start/Stop, status label with port, static note "Closing this window will shut down the server." No close-confirm modal.
- **Server Thread**: The Axum server is spawned on a Tokio runtime inside the eframe app, not a separate daemon.
- **Extension**: Shows "server not running" message. Primary diagnostic.
- **Filtering Strategy**: FP32 ONNX (split decoders) + Greedy Search + Preemptive Trie Masking + Regex Fallback.
- **Rust Implementation**: Use `ort` crate. Implement a custom Trie for masking to avoid Python's 60% masking latency penalty.

## Reuse (Migration Plan)

We are repurposing the existing `hfm-reader` code:
- **Keep**: `TextBuffer`, `PipelineController`, `types.rs`. The eframe GUI becomes the Server Control Panel.
- **Delete**: `source/local.rs` and local file UI logic.
- **Add**: `filter/detox.rs` (Rust ONNX), `server.rs` (Axum WebSocket).

## Sequencing

1. ~~Python measurement~~ (Done).
2. **The Great Rename**: Update `Cargo.toml`, workspace members, and crate name.
3. **Clean up**: Delete `source` module and local file logic from `main.rs`.
4. **Rust ONNX**: Implement `BartDetoxFilter` in Rust using `ort`, split FP32 models, Greedy Search, and a native Trie mask.
5. **The Server**: Integrate Axum, write the WebSocket handler, and wire it to the Pipeline.
6. **The Control Panel**: Update the eframe UI to start/stop the Axum server and show status.
7. **The Extension**: Write the TypeScript Chrome extension.
