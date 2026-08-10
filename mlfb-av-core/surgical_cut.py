import onnx
from onnx import helper, TensorProto

# Load the stock model
model = onnx.load("models/segmentation_gpu.onnx")

# Remove all existing outputs (keep aux? we'll just clear and add our one)
model.graph.ClearField("output")

# Define the new output: tensor "516" (latent before Resize_140)
# Shape is dynamic: batch, 21, latent_height, latent_width (factor 8 smaller)
new_output = helper.make_tensor_value_info(
    "516", TensorProto.FLOAT, ["batch", 21, "latent_height", "latent_width"]
)
model.graph.output.append(new_output)

# Save the modified model
onnx.save(model, "models/segmentation_latent.onnx")
print("SUCCESS: Created models/segmentation_latent.onnx")
