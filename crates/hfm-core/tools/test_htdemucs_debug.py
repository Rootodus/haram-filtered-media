import onnxruntime as ort
import numpy as np
import librosa
import soundfile as sf
import time

model_path = "../models/htdemucs_ft_vocals_fp16weights.onnx"
audio_path = "../assets/mixed_audio.wav"
output_base = "../assets/htdemucs_stems"

# Load model
session = ort.InferenceSession(model_path, providers=["CPUExecutionProvider"])
input_name = session.get_inputs()[0].name
input_shape = session.get_inputs()[0].shape
output_name = session.get_outputs()[0].name
print(f"Input: {input_name} -> {input_shape}")
print(f"Output: {output_name}")

expected_samples = 343980  # fixed for this model
TARGET_SR = 44100

# Load audio
audio, sr = librosa.load(audio_path, sr=TARGET_SR, mono=False)
if audio.ndim == 1:
    audio = np.stack([audio, audio])
if audio.shape[1] < expected_samples:
    pad = expected_samples - audio.shape[1]
    audio = np.pad(audio, ((0, 0), (0, pad)), mode="constant")
audio = audio[:, :expected_samples]  # (2, N)

print(
    f"Input audio stats: min={audio.min():.3f}, max={audio.max():.3f}, mean={audio.mean():.3f}"
)

# Input tensor (float32)
input_tensor = audio[np.newaxis, ...].astype(np.float32)  # (1, 2, N)

# OPTIONAL: Try fp16 input (uncomment if you want)
# input_tensor = input_tensor.astype(np.float16)

# Warm-up
for _ in range(3):
    _ = session.run([output_name], {input_name: input_tensor})

# Run inference
start = time.time()
outputs = session.run([output_name], {input_name: input_tensor})
end = time.time()
print(f"Inference time: {(end-start)*1000:.2f} ms")

separated = outputs[0]  # (1, 4, 2, N)
print("Output shape:", separated.shape)

# Save each stem
stem_names = ["vocals", "drums", "bass", "other"]
for i, name in enumerate(stem_names):
    stem = separated[0, i, :, :]  # (2, N)
    out_path = f"{output_base}_{name}.wav"
    sf.write(out_path, stem.T, TARGET_SR, subtype="FLOAT")
    # Compute RMS (loudness) to see which stem is strongest
    rms = np.sqrt(np.mean(stem**2))
    print(
        f"Stem {i} ({name}): min={stem.min():.3f}, max={stem.max():.3f}, mean={stem.mean():.3f}, RMS={rms:.4f}"
    )
    print(f"  Saved to {out_path}")

# Also save the input audio for reference
sf.write("../assets/debug_input.wav", audio.T, TARGET_SR, subtype="FLOAT")
print("Saved input audio to ../assets/debug_input.wav")
