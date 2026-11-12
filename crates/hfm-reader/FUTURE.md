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

## Prerequisites (block the pivot)

Measure in Python first:

1. Banned-substring rate of `bart-base-detox` output on ParaDetox, a realistic
   rant, and a strict banned-word test set.
2. Latency matrix: sequential greedy, sequential beam=5, 4-worker, batched-8.
   Report p50/p95 and RSS.
3. Argmax-is-masked rate: how often the unmasked argmax token contains a banned
   substring. Decides whether logit suppression is worth building.

## Open question

Logit suppression applies only to generative rewriters. If the model classifies
or redacts, suppression is irrelevant and "no banned word in output" is trivially
true. Confirm the model's output type before designing masking.

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

## Reuse

`TextSource`, `TextFilter`, `TextBuffer`, `PipelineController` survive on the
server. `LocalFileSource` becomes one source among many. eframe/egui UI does not
carry over.

## Sequencing

1. Python measurement (prerequisites above). One day.
2. Decide masking based on argmax-is-masked rate.
3. Rust parity: beam search in `BartDetoxFilter` (currently greedy — a quality gap
   today, independent of the pivot).
4. Then: extension + localhost server.