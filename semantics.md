# Playback Semantics

Open questions. Delete when answered. If the answer needs enforcing,
write a test — the test is the record.

## Open

1. **Underrun policy.** ML slow → audio silences (buffer fills zeros),
   video freezes (AudioClock stops advancing, pop gated on !buffering).
   Inconsistent. Which is intended? What if buffer never refills?

2. **Playback state.** Four enums/tables describe the lifecycle:
   reader::PipelineState, core::PipelineState (dead), coordination::
   PlaybackState, GUI AtomicU8. Which owns it? Does Seeking live inside
   the enum or alongside it? Is Buffering a state or a flag?

3. **Seek counters.** SeekGeneration (coordination.rs) vs
   MediaBuffer.flush_gen (buffer.rs). Which is authoritative? GStreamer::seek
   fires before the generation bump in another thread — race?

4. **Seek paths.** Keyboard seek resets audio_clock + sets clear flag;
   button seek only resets sync_state. Same or different?

5. **Reader cmd_rx.** bounded(16), cloned into nothing, no try_recv.
   17th send blocks the UI thread. Delete or wire?

## Answered

(nothing yet)