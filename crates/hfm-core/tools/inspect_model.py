import onnx

# uv run python -c "import urllib.request; urllib.request.urlretrieve('https://huggingface.co/opencv/human_segmentation_pphumanseg/resolve/main/human_segmentation_pphumanseg_2023mar.onnx', 'models/pphumanseg.onnx')"
model = onnx.load("../models/pphumanseg.onnx")

print("=== Model Info ===")
print(f"IR Version: {model.ir_version}")
print(f"Opset: {model.opset_import[0].version}")
print(f"Producer: {model.producer_name}")

print("\n=== Inputs ===")
for inp in model.graph.input:
    print(
        f"  {inp.name}: {inp.type.tensor_type.elem_type} (shape: {inp.type.tensor_type.shape.dim}"
    )

print("\n=== Outputs ===")
for out in model.graph.output:
    print(
        f"  {out.name}: {out.type.tensor_type.elem_type} (shape: {out.type.tensor_type.shape.dim}"
    )

print("\n=== Operators ===")
ops = set()
for node in model.graph.node:
    ops.add(node.op_type)
print(f"Unique ops: {sorted(ops)}")

# Optional: count parameters
init_tensors = {init.name: init.dims for init in model.graph.initializer}
print(f"\nNumber of initializers (weights): {len(init_tensors)}")
print(f"Sample weight shapes: {list(init_tensors.values())[:5]}")
