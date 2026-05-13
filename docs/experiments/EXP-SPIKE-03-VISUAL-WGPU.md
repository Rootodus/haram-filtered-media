# Experiment: WGPU Presentation & Backpressure
ID: EXP-SPIKE-03  
Status: SUCCESS

## Hypothesis
A dedicated Renderer thread using `wgpu` can display pixel buffers with "Stop-and-Wait" backpressure to prevent buffer bloat.

## Evidence
- Hardware: Intel Iris Xe (Vulkan Backend).
- Presentation Mode: `Fifo` (V-Sync enabled).
- Result: Average FPS locked to ~25-30 due to monitor refresh sync and hard-sync ACK logic.
- Visuals: Confirmed "pulse" synchronization; color changes matched JS-sent timestamps.

## Conclusion
The graphics stack is functional. Admission control correctly skips frames when the GPU/Monitor is busy, satisfying `MODE-LATENCY`.
