# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "onnxruntime",
#     "onnx",
# ]
# ///

from pathlib import Path
from onnxruntime.quantization import QuantType, quantize_dynamic
from onnxruntime.quantization.shape_inference import quant_pre_process

MODEL_DIR = Path(__file__).parent.parent / "models" / "bart-base-detox" / "onnx"

# Target the standalone decoders instead of the merged one
MODELS_TO_QUANTIZE = ["encoder_model", "decoder_model", "decoder_with_past_model"]

def quantize_model(model_name: str):
    fp32_path = MODEL_DIR / f"{model_name}.onnx"
    int8_path = MODEL_DIR / f"{model_name}_int8.onnx"
    pre_processed_path = MODEL_DIR / f"_{model_name}.pre.onnx"

    if not fp32_path.exists():
        print(f"Skipping {model_name}: {fp32_path} not found.")
        return

    print(f"\nQuantizing {model_name}...")

    try:
        quant_pre_process(
            str(fp32_path),
            str(pre_processed_path),
            skip_symbolic_shape=True,
            save_as_external_data=True,
            all_tensors_to_one_file=True,
            external_data_location=f"_{model_name}.pre.onnx_data",
        )
        source_path = pre_processed_path
    except Exception as e:
        print(f"  Pre-processing skipped ({e}). Using original file.")
        source_path = fp32_path

    quantize_dynamic(
        str(source_path),
        str(int8_path),
        weight_type=QuantType.QInt8,
        extra_options={"MatMulConstBOnly": True},
    )

    if pre_processed_path.exists():
        pre_processed_path.unlink()
    external_data = MODEL_DIR / f"_{model_name}.pre.onnx_data"
    if external_data.exists():
        external_data.unlink()

    print(f"  Created: {int8_path.name} ({int8_path.stat().st_size / 1_048_576:.1f} MB)")

if __name__ == "__main__":
    for model in MODELS_TO_QUANTIZE:
        quantize_model(model)

    print("\nQuantization complete. The `_int8.onnx` files are ready for inference.")
