import onnx

model = onnx.load("models/segmentation_latent.onnx")

print("=== Outputs ===")
for out in model.graph.output:
    print(f"  {out.name}")

print("\n=== All Resize Nodes ===")
for node in model.graph.node:
    if node.op_type == "Resize":
        print(f"  {node.name}: inputs={node.input}, outputs={node.output}")

print("\n=== Total nodes ===")
print(f"Number of nodes: {len(model.graph.node)}")
