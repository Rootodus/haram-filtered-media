import onnxruntime as ort
import numpy as np

providers = ["DmlExecutionProvider"]  # no CPU fallback

# https://huggingface.co/StemSplitio/htdemucs-ft-vocals-onnx/blob/main/htdemucs_ft_vocals_fp16weights.onnx
session = ort.InferenceSession(
    "../models/htdemucs_ft_vocals_fp16weights.onnx", providers=providers
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
