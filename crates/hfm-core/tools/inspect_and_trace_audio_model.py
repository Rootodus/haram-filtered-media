import onnx
import onnxruntime as ort
import numpy as np
import collections

model_path = "../models/htdemucs_ft_vocals.onnx"

# 1. Profile operator frequencies to see the structural complexity
model = onnx.load(model_path)
op_counts = collections.Counter([node.op_type for node in model.graph.node])
print("--- Operator Count Breakdown ---")
for op, count in op_counts.most_common():
    print(f"{op}: {count}")

# 2. Configure ORT to log exactly what it is trying to compile right before it dies
print("\n--- Starting DirectML Initialization Trace ---")
sess_options = ort.SessionOptions()
sess_options.log_severity_level = 0  # 0 = Verbose logging level
sess_options.log_verbosity_level = 1

# If it crashes on compilation, toggle this to see if isolation helps
sess_options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_DISABLE_ALL

try:
    # Force DirectML Execution Provider
    session = ort.InferenceSession(
        model_path, sess_options, providers=["DmlExecutionProvider"]
    )
    print("SUCCESS: Model compiled on DirectML without crashing!")

    # Run a tiny 1-frame dummy pass to trace execution
    input_name = session.get_inputs()[0].name
    input_shape = session.get_inputs()[0].shape

    # Replace dynamic batch/duration string dimensions with 1 or small integers
    static_shape = [
        1 if isinstance(dim, str) or dim is None else dim for dim in input_shape
    ]
    dummy_input = np.random.randn(*static_shape).astype(np.float32)

    print("Running a single speculative execution frame...")
    session.run(None, {input_name: dummy_input})
    print("Execution finished successfully.")

except Exception as e:
    print(f"Handled error caught: {e}")
