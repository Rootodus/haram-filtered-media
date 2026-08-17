import onnxruntime as ort
import numpy as np
import librosa
import soundfile as sf
import time
import argparse


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--model",
        default="../models/htdemucs_ft_vocals_fp16weights.onnx",
        help="Path to ONNX model",
    )
    parser.add_argument(
        "--input", default="../assets/mixed_audio.wav", help="Input audio file"
    )
    parser.add_argument(
        "--output",
        default="../assets/speech_output.wav",
        help="Output file for extracted speech",
    )
    parser.add_argument(
        "--stem",
        type=int,
        default=3,
        choices=[0, 1, 2, 3],
        help="Stem to extract: 0=vocals, 1=drums, 2=bass, 3=other (default: 3)",
    )
    parser.add_argument(
        "--backend", default="cpu", choices=["cpu", "dml"], help="Execution provider"
    )
    parser.add_argument(
        "--sample-rate", type=int, default=44100, help="Target sample rate (Hz)"
    )
    parser.add_argument(
        "--window-samples",
        type=int,
        default=343980,
        help="Number of samples per channel",
    )
    args = parser.parse_args()

    # Build provider list
    providers = (
        ["DmlExecutionProvider"] if args.backend == "dml" else ["CPUExecutionProvider"]
    )

    # Load model
    session = ort.InferenceSession(args.model, providers=providers)
    print("Providers:", session.get_providers())
    input_name = session.get_inputs()[0].name
    output_name = session.get_outputs()[0].name
    print(f"Input: {input_name} -> {session.get_inputs()[0].shape}")
    print(f"Output: {output_name} -> {session.get_outputs()[0].shape}")

    expected_samples = args.window_samples
    TARGET_SR = args.sample_rate

    # Load and prepare audio
    audio, sr = librosa.load(args.input, sr=TARGET_SR, mono=False)
    if audio.ndim == 1:
        audio = np.stack([audio, audio])  # mono → stereo
    # Pad/trim to expected length
    if audio.shape[1] < expected_samples:
        pad = expected_samples - audio.shape[1]
        audio = np.pad(audio, ((0, 0), (0, pad)), mode="constant")
    audio = audio[:, :expected_samples]  # (channels, samples)

    print(
        f"Input stats: min={audio.min():.3f}, max={audio.max():.3f}, mean={audio.mean():.3f}"
    )

    # Input tensor: (batch, channels, samples)
    input_tensor = audio[np.newaxis, ...].astype(np.float32)

    # Warm‑up
    for _ in range(3):
        _ = session.run([output_name], {input_name: input_tensor})

    # Benchmark (optional)
    start = time.time()
    outputs = session.run([output_name], {input_name: input_tensor})
    end = time.time()
    print(f"Inference time: {(end-start)*1000:.2f} ms")

    separated = outputs[0]  # (1, 4, 2, N)
    print(f"Output shape: {separated.shape}")

    # Extract the chosen stem
    stem = separated[0, args.stem, :, :]  # (channels, samples)
    print(
        f"Stem {args.stem} stats: min={stem.min():.3f}, max={stem.max():.3f}, mean={stem.mean():.3f}"
    )

    # Save output
    sf.write(args.output, stem.T, TARGET_SR, subtype="FLOAT")
    print(f"Saved to {args.output}")


if __name__ == "__main__":
    main()
