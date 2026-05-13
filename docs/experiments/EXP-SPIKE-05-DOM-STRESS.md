# Experiment: DOM Stress Test (MessagePack)
ID: EXP-SPIKE-05  
Status: FAILED

## Hypothesis
MessagePack can deserialize a 5,000-node DOM tree within a 16.6ms frame budget.

## Evidence
- DOM Size: 5,000 nodes (~278KB).
- Borrowed (Zero-Alloc) Result: ~11ms.
- Owned (Heap-Alloc) Result: ~14ms.
- Jitter: Spikes up to 124ms (Rust) and 185ms (Node.js).
- Total Latency: ~75ms average (~13 FPS).

## Analysis
The bottleneck is not allocation, but Sequential Scanning. The CPU must walk every byte of the buffer to decode the MessagePack structure, consuming 66% of the frame budget before GPU upload or Inference begins.

## Conclusion
MessagePack is REJECTED for structural data. Transitioning to FlatBuffers for O(1) random access.
