import onnxruntime as ort
import numpy as np
import librosa
import soundfile as sf
import time

model_path = "../models/htdemucs_ft_vocals_fp16weights.onnx"
audio_path = "../assets/mixed_audio.wav"
output_path = "../assets/htdemucs_vocals_output.wav"

# 1. Load model
session = ort.InferenceSession(model_path, providers=["CPUExecutionProvider"])
input_name = session.get_inputs()[0].name
input_shape = session.get_inputs()[0].shape
output_name = session.get_outputs()[0].name
print(f"Input: {input_name} -> {input_shape}")
print(f"Output: {output_name}")

expected_samples = 343980  # from earlier

# 2. Load audio (44.1 kHz, stereo)
TARGET_SR = 44100
audio, sr = librosa.load(audio_path, sr=TARGET_SR, mono=False)
if audio.ndim == 1:
    audio = np.stack([audio, audio])
# Trim/pad
if audio.shape[1] < expected_samples:
    pad = expected_samples - audio.shape[1]
    audio = np.pad(audio, ((0, 0), (0, pad)), mode="constant")
audio = audio[:, :expected_samples]  # (2, N)

print(
    "Input audio stats: min={:.3f}, max={:.3f}, mean={:.3f}".format(
        audio.min(), audio.max(), audio.mean()
    )
)

# 3. Prepare input tensor (float32)
input_tensor = audio[np.newaxis, ...].astype(np.float32)  # (1, 2, N)

# 4. Warm-up
for _ in range(3):
    _ = session.run([output_name], {input_name: input_tensor})

# 5. Run inference and measure
start = time.time()
outputs = session.run([output_name], {input_name: input_tensor})
end = time.time()
print(f"Inference time: {(end-start)*1000:.2f} ms")

separated = outputs[0]  # (1, 4, 2, N)
print("Output shape:", separated.shape)
print(
    "Output stats: min={:.3f}, max={:.3f}, mean={:.3f}".format(
        separated.min(), separated.max(), separated.mean()
    )
)

# Extract vocals (stem 0)
vocals = separated[0, 0, :, :]  # (2, N)
print(
    "Vocals stats: min={:.3f}, max={:.3f}, mean={:.3f}".format(
        vocals.min(), vocals.max(), vocals.mean()
    )
)

# 6. Save vocals
sf.write(output_path, vocals.T, TARGET_SR, subtype="FLOAT")
print(f"Saved to {output_path}")

# Also save the input audio for comparison
sf.write("../assets/debug_input.wav", audio.T, TARGET_SR, subtype="FLOAT")
print("Saved input audio to ../assets/debug_input.wav")
