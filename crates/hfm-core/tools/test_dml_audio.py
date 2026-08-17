import onnxruntime as ort
import numpy as np

providers = ["DmlExecutionProvider"]  # no CPU fallback

# https://github.com/k2-fsa/sherpa-onnx/releases/download/source-separation-models/sherpa-onnx-spleeter-2stems-fp16.tar.bz2
session = ort.InferenceSession(
    "../models/sherpa-onnx-spleeter-2stems-fp16/vocals.fp16.onnx", providers=providers
)

print("Providers:", session.get_providers())

# Use the shape from inspection (example: [1, 2, 16000])
input_name = session.get_inputs()[0].name
input_shape = session.get_inputs()[0].shape  # could be dynamic, use explicit shape

# If shape has -1 (dynamic), replace with concrete value
fixed_shape = [dim if dim != -1 else 16000 for dim in input_shape]
dummy = np.random.randn(*fixed_shape).astype(np.float32)

outputs = session.run(None, {input_name: dummy})
print("Output shape:", outputs[0].shape)
