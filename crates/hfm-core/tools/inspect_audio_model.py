import onnx

# https://github.com/k2-fsa/sherpa-onnx/releases/download/source-separation-models/sherpa-onnx-spleeter-2stems-fp16.tar.bz2
model = onnx.load("../models/sherpa-onnx-spleeter-2stems-fp16/vocals.fp16.onnx")

print("=== Model Info ===")
print(f"IR Version: {model.ir_version}")
print(f"Opset: {model.opset_import[0].version}")
print(f"Producer: {model.producer_name}")

print("\n=== Inputs ===")
for inp in model.graph.input:
    shape = [dim.dim_value for dim in inp.type.tensor_type.shape.dim]
    print(f"  {inp.name}: {inp.type.tensor_type.elem_type} (shape: {shape})")

print("\n=== Outputs ===")
for out in model.graph.output:
    shape = [dim.dim_value for dim in out.type.tensor_type.shape.dim]
    print(f"  {out.name}: {out.type.tensor_type.elem_type} (shape: {shape})")

print("\n=== Operators ===")
ops = set()
for node in model.graph.node:
    ops.add(node.op_type)
print(f"Unique ops: {sorted(ops)}")

print("\n=== Metadata ===")
for entry in model.metadata_props:
    print(f"  {entry.key}: {entry.value}")
