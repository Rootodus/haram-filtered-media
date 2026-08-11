# Experiment: Raw Pixel Pipe Throughput
ID: EXP-SPIKE-02-PIXEL-PIPE  
Status: SUCCESS  
Depends on: @STD-DOC, @EXP-RULES

## Hypothesis
A TCP loopback pipe using Header-Payload separation can move 1080p RGBA8 frames at >60 FPS.

## Evidence
- Environment: Intel Iris Xe (Unified Memory), Windows 11.
- Payload: 8.3 MB (1920x1080x4).
- Result: ~125 FPS observed in JS Loader.
- Latency: 3-5 ms (Net IO).

## Conclusion
TCP loopback is sufficient for the video data plane; Shared Memory (SHM) is deferred to Endgame.
