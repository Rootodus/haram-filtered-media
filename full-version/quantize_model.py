import onnx
from onnxconverter_common import float16

# Load the model
model_path = "model.onnx"
output_path = "model_fp16.onnx"

print(f"Loading model from {model_path}...")
model = onnx.load(model_path)

print("Converting model weights and operations to FP16...")
# This explicitly converts FP32 weights and node signatures to FP16
model_fp16 = float16.convert_float_to_float16(model)

# Save the newly packed model
onnx.save(model_fp16, output_path)
print(f"FP16 model successfully saved to: {output_path}")
