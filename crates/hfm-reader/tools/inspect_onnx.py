# /// script
# requires-python = ">=3.8"
# dependencies = [
#     "onnxruntime",
# ]
# ///

import onnxruntime as ort
from pathlib import Path

def inspect_model(path):
    print(f"\n=== {path.name} ===")
    session = ort.InferenceSession(str(path))
    for inp in session.get_inputs():
        print(f"Input:  {inp.name}, shape: {inp.shape}, type: {inp.type}")
    for out in session.get_outputs():
        print(f"Output: {out.name}, shape: {out.shape}, type: {out.type}")

if __name__ == "__main__":
    base = Path("./models/bart-base-detox/onnx")
    inspect_model(base / "encoder_model.onnx")
    inspect_model(base / "decoder_model_merged.onnx")