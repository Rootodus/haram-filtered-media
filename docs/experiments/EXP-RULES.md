# Experiment & Spike Logging Rules [NORMATIVE]
ID: EXP-RULES  
Status: STABLE  
Depends on: @STD-DOC

## Purpose
Defines how technical spikes and performance benchmarks MUST be documented to provide evidence for architectural decisions.

## Required Structure
Each experiment file MUST follow the "HEAC" pattern:
1. Hypothesis: What technical risk or performance goal is being tested?
2. Evidence:
   - Hardware/Software environment (e.g., Intel Iris Xe, Windows 11).
   - Quantitative data (FPS, Latency in ms, CPU/GPU % usage).
   - Representative log snippets (Max 20-50 lines; do not store raw bitstreams).
3. Analysis: Why did the numbers look this way? Identify bottlenecks (CPU, IO, Memory, V-Sync).
4. Conclusion: What ARCH-REQ or DECISION does this experiment trigger?

## Metrics Integrity
- Metrics MUST be measured using high-precision timers (`std::time::Instant` or `performance.now()`).
- Outliers (Jitter) MUST be reported, not averaged out.

## Trace Handling
- Raw binary payloads (Pixels/DOM buffers) MUST NOT be stored in the experiment logs.
- Summaries (Size in KB, Checksums) are preferred.

## Validation Rule
An experiment is VALID only if it provides a clear signal for a DECISION or a GAP resolution.

## Notes / Explanatory
- [EXPLANATORY] The following experiments were written before this document: @EXP-SPIKE-01-IPC-PROTO, @EXP-SPIKE-02-PIXEL-PIPE, @EXP-SPIKE-03-VISUAL-WGPU, @EXP-SPIKE-04-PRESSURE-TEST, @EXP-SPIKE-05-DOM-STRESS
