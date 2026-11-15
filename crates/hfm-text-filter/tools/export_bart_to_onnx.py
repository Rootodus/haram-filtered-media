# /// script
# requires-python = ">=3.8"
# dependencies = [
#     "transformers",
#     "torch",
#     "optimum-onnx",
#     "onnxruntime",
# ]
# ///

from pathlib import Path
from optimum.exporters.onnx import main_export

def main():
    model_path = Path("./models/bart-base-detox")
    output_path = model_path / "onnx"

    if not model_path.exists():
        raise FileNotFoundError(f"Model directory not found: {model_path}")

    print("Exporting to ONNX via Hugging Face Optimum...")
    
    main_export(
        model_name_or_path=str(model_path),
        output=str(output_path),
        task="seq2seq-lm-with-past",
        opset=18,
    )

    print(f"✅ ONNX model components successfully exported to: {output_path}")

if __name__ == "__main__":
    main()
