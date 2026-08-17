import onnxruntime as ort
import numpy as np
import librosa
import soundfile as sf
import time

# Paths
model_path = "../models/htdemucs_ft_vocals_fp16weights.onnx"
audio_path = "../assets/mixed_audio.wav"
output_path = "../assets/htdemucs_vocals_output.wav"

# 1. Load model and inspect
session = ort.InferenceSession(model_path, providers=["CPUExecutionProvider"])
print("Providers:", session.get_providers())

input_name = session.get_inputs()[0].name
input_shape = session.get_inputs()[0].shape
output_name = session.get_outputs()[0].name
output_shape = session.get_outputs()[0].shape
print(f"Input: {input_name} -> {input_shape}")
print(f"Output: {output_name} -> {output_shape}")

# Determine expected samples (last dimension)
expected_samples = input_shape[-1]
if expected_samples == -1:
    expected_samples = 343980  # typical for HT-Demucs
    print(f"Using default expected_samples = {expected_samples}")
else:
    print(f"Expected samples per channel: {expected_samples}")

# 2. Load and resample audio
TARGET_SR = 44100  # HT-Demucs expects 44.1 kHz
audio, sr = librosa.load(audio_path, sr=TARGET_SR, mono=False)
if audio.ndim == 1:
    audio = np.stack([audio, audio])  # mono → stereo

# Pad/trim to expected length
total_samples = expected_samples
if audio.shape[1] < total_samples:
    pad_len = total_samples - audio.shape[1]
    audio = np.pad(audio, ((0, 0), (0, pad_len)), mode="constant")
audio = audio[:, :total_samples]  # (2, samples)

# 3. Prepare input tensor: (batch, channels, samples)
input_tensor = audio[np.newaxis, ...].astype(np.float32)  # (1, 2, samples)

# 4. Warm-up
for _ in range(3):
    _ = session.run([output_name], {input_name: input_tensor})

# 5. Benchmark
n_runs = 10
start = time.time()
for _ in range(n_runs):
    _ = session.run([output_name], {input_name: input_tensor})
end = time.time()
avg_ms = (end - start) / n_runs * 1000
print(f"Avg inference time: {avg_ms:.2f} ms")
window_duration = expected_samples / TARGET_SR
rtf = avg_ms / 1000 / window_duration
print(f"Window duration: {window_duration:.2f}s, RTF: {rtf:.4f}")

# 6. Run inference
outputs = session.run([output_name], {input_name: input_tensor})
separated = outputs[0]  # shape (1, 4, 2, samples) – 4 stems
vocals = separated[0, 0, :, :]  # extract first stem (vocals) -> (2, samples)

# 7. Save output
sf.write(output_path, vocals.T, TARGET_SR, subtype="FLOAT")
print(f"Saved separated vocals to {output_path}")
