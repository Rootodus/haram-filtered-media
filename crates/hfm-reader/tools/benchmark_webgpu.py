# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "onnxruntime>=1.24.4",
#     "onnxruntime-ep-webgpu",
#     "numpy",
# ]
# ///

import time
from pathlib import Path
import numpy as np
import onnxruntime as ort
import onnxruntime_ep_webgpu as webgpu_ep

# Register the WebGPU plugin
ort.register_execution_provider_library("webgpu", webgpu_ep.get_library_path())

MODEL_PATH = Path(__file__).parent.parent / "models" / "bart-base-detox" / "onnx" / "encoder_model.onnx"


def benchmark(provider_name, session_options):
    print(f"\n--- Benchmarking {provider_name} ---")
    
    # 1. Create Session
    # If using WebGPU, we fall back to CPU if the GPU hits a dead end or fails
    providers = [provider_name]
    if provider_name == "WebGpuExecutionProvider":
        providers.append("CPUExecutionProvider")
        
    try:
        session = ort.InferenceSession(str(MODEL_PATH), session_options, providers=providers)
    except Exception as e:
        print(f"Failed to create session: {e}")
        return float('inf')

    # 2. Verify active provider
    active_providers = session.get_providers()
    print(f"Active providers: {active_providers}")
    if active_providers[0] != provider_name:
        print(f"WARNING: {provider_name} failed to activate. Running on {active_providers[0]}.")

    # 3. Create dummy input (BART encoder expects input_ids and attention_mask)
    # Shape: [batch_size, sequence_length] -> 1 batch, 20 tokens
    input_ids = np.ones((1, 20), dtype=np.int64)
    attention_mask = np.ones((1, 20), dtype=np.int64)
    inputs = {
        "input_ids": input_ids,
        "attention_mask": attention_mask
    }

    # 4. Warmup (crucial for GPU, first run initializes VRAM buffers)
    for _ in range(5):
        session.run(None, inputs)

    # 5. Benchmark
    runs = 30
    start = time.perf_counter()
    for _ in range(runs):
        session.run(None, inputs)
    end = time.perf_counter()
    
    avg_ms = ((end - start) / runs) * 1000
    print(f"Average time: {avg_ms:.2f} ms per forward pass")
    return avg_ms


if __name__ == "__main__":
    if not MODEL_PATH.exists():
        raise FileNotFoundError(f"Could not find ONNX model at {MODEL_PATH}")

    opts = ort.SessionOptions()
    opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL

    # Run CPU Benchmark
    cpu_ms = benchmark("CPUExecutionProvider", opts)

    # Run WebGPU Benchmark
    webgpu_ms = benchmark("WebGpuExecutionProvider", opts)

    # Compare
    print("\n" + "=" * 40)
    print("CONCLUSION")
    print("=" * 40)
    if webgpu_ms < cpu_ms:
        speedup = cpu_ms / webgpu_ms
        print(f"WebGPU is {speedup:.2f}x FASTER than CPU.")
        print("The iGPU/WebGPU path is viable.")
    else:
        slowdown = webgpu_ms / cpu_ms
        print(f"WebGPU is {slowdown:.2f}x SLOWER than CPU.")
        print("The overhead of moving tensors to the GPU is not worth it for this model size.")
        print("Stick to CPU (and implement Int8 quantization).")
