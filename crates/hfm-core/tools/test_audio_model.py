import onnxruntime as ort
import numpy as np
import time
import argparse
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", required=True, help="Path to ONNX model")
    parser.add_argument(
        "--backend", default="cpu", choices=["cpu", "dml"], help="Execution provider"
    )
    parser.add_argument("--input", help="Input audio file (WAV) for quality test")
    parser.add_argument("--output", help="Output file for separated vocals (WAV)")
    parser.add_argument(
        "--sample-rate", type=int, default=44100, help="Sample rate (Hz)"
    )
    args = parser.parse_args()

    # Build provider list
    if args.backend == "dml":
        providers = ["DmlExecutionProvider"]
    else:
        providers = ["CPUExecutionProvider"]

    # Load model
    session = ort.InferenceSession(args.model, providers=providers)
    print("Providers:", session.get_providers())

    # Get input info
    input_info = session.get_inputs()[0]
    input_name = input_info.name
    input_shape = input_info.shape  # e.g., ['batch', 'channels', 'samples']
    print(f"Input: {input_name} -> {input_shape}")

    # Resolve dynamic dimensions: set batch=1, if samples is -1 use default 343980 (common for HT-Demucs)
    fixed_shape = []
    for dim in input_shape:
        if isinstance(dim, int) and dim > 0:
            fixed_shape.append(dim)
        else:
            # For batch, use 1; for samples, use a default (will be overridden if real input is used)
            fixed_shape.append(1 if dim == "batch" or dim == 0 else 343980)
    print(f"Fixed shape for dummy: {fixed_shape}")

    # Create dummy input
    dummy = np.random.randn(*fixed_shape).astype(np.float32)

    # Warm-up
    for _ in range(3):
        session.run(None, {input_name: dummy})

    # Benchmark (dummy)
    n_runs = 20
    start = time.time()
    for _ in range(n_runs):
        session.run(None, {input_name: dummy})
    end = time.time()
    avg_ms = (end - start) / n_runs * 1000
    print(f"Avg dummy inference time: {avg_ms:.2f} ms")

    # Compute RTF (using fixed sample rate and window length)
    # For HT-Demucs, the model processes a fixed window of length fixed_shape[-1] samples.
    window_duration = fixed_shape[-1] / args.sample_rate
    rtf = avg_ms / 1000 / window_duration
    print(f"Window duration: {window_duration:.2f}s")
    print(f"RTF: {rtf:.4f} (inference time / window duration)")
    if rtf < 0.1:
        print("✅ RTF < 0.1 – suitable for real-time.")
    else:
        print("❌ RTF >= 0.1 – may be too slow.")

    # If input and output files are provided, process real audio
    if args.input and args.output:
        try:
            import librosa
            import soundfile as sf

            audio, sr = librosa.load(args.input, sr=args.sample_rate, mono=False)
            if audio.ndim == 1:
                audio = np.stack([audio, audio])  # mono->stereo
            # Ensure we have the right number of samples (pad/trim)
            expected_samples = fixed_shape[-1]
            if audio.shape[1] < expected_samples:
                pad_len = expected_samples - audio.shape[1]
                audio = np.pad(audio, ((0, 0), (0, pad_len)), mode="constant")
            audio = audio[:, :expected_samples]
            # Prepare input: (1, channels, samples)
            input_tensor = audio[np.newaxis, ...].astype(np.float32)
            # Run inference
            outputs = session.run(None, {input_name: input_tensor})
            separated = outputs[0]  # (1, channels, samples)
            separated = separated[0]  # (channels, samples)
            # Save
            sf.write(args.output, separated.T, sr, subtype="FLOAT")
            print(f"Saved separated audio to {args.output}")
        except ImportError:
            print(
                "librosa and soundfile required for quality test. Install with: uv add librosa soundfile"
            )
        except Exception as e:
            print(f"Error processing real audio: {e}")


if __name__ == "__main__":
    main()
