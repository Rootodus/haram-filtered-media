# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "onnxruntime>=1.24.4",
#     "onnxruntime-ep-webgpu",
# ]
# ///

import onnxruntime as ort
import onnxruntime_ep_webgpu as webgpu_ep

ort.register_execution_provider_library("webgpu", webgpu_ep.get_library_path())

devices = ort.get_ep_devices()
webgpu_devices = [d for d in devices if d.ep_name == webgpu_ep.get_ep_name()]

print(f"Available providers: {ort.get_available_providers()}")
print(f"WebGPU devices found: {len(webgpu_devices)}")

if not webgpu_devices:
    raise RuntimeError("No WebGPU device found. Check your Vulkan installation.")
else:
    for i, device in enumerate(webgpu_devices):
        print(f"  [{i}] {device.ep_name} (Vendor: {device.ep_vendor})")
