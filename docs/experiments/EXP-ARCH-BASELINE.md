# Experiment: Foundational Pivot (Simulation vs. Native)
ID: EXP-ARCH-BASELINE  
Status: STABLE  
Depends on: STD-DOC, EXP-RULES

## Hypothesis
Browser extensions and localhost servers introduce overhead incompatible with real-time ML performance requirements. A formal simulation/harness model introduces excessive abstraction debt; a direct "Spike-First" native implementation is required to validate Native Performance.

## Evidence
- Browser Extension: Qualitatively observed as "slow".
- Localhost Server: Observed to have "latency-like network", introducing unacceptable delay for frame-sync.
- Methodological Failure: Initial attempts to define a discrete-step simulation harness resulted in "Document Drift" and logical contradictions regarding batching vs. atomicity.
- Quantitative Loss: Logs from the old simulation phase were deleted as they measured "Simulation Overhead" rather than "Native Throughput".

## Analysis
The "wrong things" were being measured in the simulation phase. The focus on logical purity in a harness delayed the discovery of physical bottlenecks (like MessagePack scanning). Standard web-interfacing methods (Extensions/HTTP) failed the "Native Performance" feel required for the project.

## Conclusion
1. REJECTED: Browser extensions and decoupled localhost servers for the core pipeline.
2. REJECTED: The discrete-step simulation harness model.
3. DECIDED: "Spike-First" native implementation.
4. DECIDED: Monolithic multithreaded architecture (`DEC-PIPE-MONOLITH`).
5. DECIDED: Hardware-clocked synchronization (`DEC-HARD-SYNC-PIPE`).

## Notes / Explanatory
- [EXPLANATORY] This document marks the boundary between the "Design-Heavy" attempt and the "Performance-Heavy" current implementation.
