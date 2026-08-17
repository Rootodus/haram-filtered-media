import onnxruntime as ort
import numpy as np

# Force DirectML only, no CPU fallback
providers = ["DmlExecutionProvider"]  # CPU not included

# https://huggingface.co/opencv/human_segmentation_pphumanseg/blob/main/human_segmentation_pphumanseg_2023mar.onnx
session = ort.InferenceSession("../models/pphumanseg.onnx", providers=providers)

print("Providers:", session.get_providers())

# Dummy input with static shape (1,3,384,640)
input_name = session.get_inputs()[0].name
dummy = np.random.randn(1, 3, 384, 640).astype(np.float32)

outputs = session.run(None, {input_name: dummy})
print("Output shape:", outputs[0].shape)
