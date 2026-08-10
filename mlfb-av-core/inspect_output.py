import onnx

# uv run python -c "import urllib.request; urllib.request.urlretrieve('https://github.com/onnx/models/raw/main/validated/vision/object_detection_segmentation/fcn/model/fcn-resnet50-12.onnx', 'models/segmentation_gpu.onnx')"
model = onnx.load("models/segmentation_gpu.onnx")

# Find all Resize nodes
resize_nodes = [node for node in model.graph.node if node.op_type == "Resize"]
print(f"Found {len(resize_nodes)} Resize nodes.")

# Check if any Resize node directly feeds into the output
output_name = model.graph.output[0].name  # "segment"
for node in resize_nodes:
    if output_name in node.output:
        print(f"✅ Resize node '{node.name}' is the final output node.")
        print(f"   Input to this Resize: {node.input[0]}")
        # Find its shape
        input_name = node.input[0]
        for vi in model.graph.value_info:
            if vi.name == input_name:
                print(f"   Latent shape: {vi.type.tensor_type.shape}")
                break
        break
else:
    print("❌ No Resize node directly feeds into the output.")
    print("   The Resize nodes may be internal (e.g., skip connections).")
    print("   Graph surgery is more complex and NOT recommended.")
