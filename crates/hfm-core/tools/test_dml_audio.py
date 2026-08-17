import onnxruntime as ort
import numpy as np
import time

# Use CPU for now (DML not available in this env)
providers = ["CPUExecutionProvider"]


# https://github.com/k2-fsa/sherpa-onnx/releases/download/source-separation-models/sherpa-onnx-spleeter-2stems-fp16.tar.bz2
session = ort.InferenceSession(
    "../models/sherpa-onnx-spleeter-2stems-fp16/vocals.fp16.onnx", providers=providers
)

print("Providers:", session.get_providers())

input_name = session.get_inputs()[0].name
input_shape = session.get_inputs()[0].shape
print("Raw input shape:", input_shape)

# Replace any dynamic dimension with 1
fixed_shape = []
for dim in input_shape:
    if isinstance(dim, int) and dim > 0:
        fixed_shape.append(dim)
    else:
        # Replace with 1 (batch size, or any unknown dim)
        fixed_shape.append(1)
print("Fixed shape:", fixed_shape)

dummy = np.random.randn(*fixed_shape).astype(np.float32)

# Warm-up
for _ in range(5):
    _ = session.run(None, {input_name: dummy})

# Measure inference time
num_runs = 100
start = time.time()
for _ in range(num_runs):
    _ = session.run(None, {input_name: dummy})
end = time.time()

avg_time_ms = (end - start) / num_runs * 1000
print(f"Average inference time: {avg_time_ms:.2f} ms")

# Get output shape
outputs = session.run(None, {input_name: dummy})
print("Output shape:", outputs[0].shape)
