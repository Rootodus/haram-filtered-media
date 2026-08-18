import onnxruntime as ort
import numpy as np
import time

model_path = "../models/htdemucs_ft_vocals.onnx"

print("--- Initializing OpenVINO Execution Provider ---")
sess_options = ort.SessionOptions()
sess_options.log_severity_level = 0  # Verbose
sess_options.log_verbosity_level = 1

# Configure OpenVINO to explicitly utilize your Iris Xe iGPU
provider_options = [
    {
        "device_type": "GPU",  # Force Intel Graphics processing
        "num_of_threads": "4",  # Balance resource pipelines
        "cache_dir": "./ov_cache",  # Cache compiled blobs to avoid long cold starts
    }
]

try:
    # Initialize the session using the true OpenVINO string identifier
    session = ort.InferenceSession(
        model_path,
        sess_options,
        providers=["OpenVINOExecutionProvider"],
        provider_options=provider_options,
    )

    # Confirm it actually bound to OpenVINO instead of silently dropping to CPU
    print(f"Active Providers: {session.get_providers()}")
    if "OpenVINOExecutionProvider" not in session.get_providers():
        print("Warning: Fallen back to CPU automatically!")
    else:
        print("SUCCESS: OpenVINO GPU backend compiled cleanly.")

    # Execute 1 mock frame to verify stability under load
    input_name = session.get_inputs()[0].name
    input_shape = session.get_inputs()[0].shape
    static_shape = [
        1 if isinstance(dim, str) or dim is None else dim for dim in input_shape
    ]
    dummy_input = np.random.randn(*static_shape).astype(np.float32)

    print("Running frame inference...")
    start = time.perf_counter()
    session.run(None, {input_name: dummy_input})
    end = time.perf_counter()

    print(f"Inference complete! Elapsed time: {end - start:.4f} seconds")

except Exception as e:
    print(f"\nExecution Failed: {e}")
