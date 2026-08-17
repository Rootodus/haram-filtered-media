import onnxruntime as ort
import numpy as np
import librosa
import soundfile as sf
import time

model_path = "../models/htdemucs_ft_vocals_fp16weights.onnx"
audio_path = "../assets/mixed_audio.wav"
output_path = "../assets/speech_output.wav"

session = ort.InferenceSession(model_path, providers=["CPUExecutionProvider"])
input_name = session.get_inputs()[0].name
output_name = session.get_outputs()[0].name
expected_samples = 343980
TARGET_SR = 44100

audio, sr = librosa.load(audio_path, sr=TARGET_SR, mono=False)
if audio.ndim == 1:
    audio = np.stack([audio, audio])
if audio.shape[1] < expected_samples:
    pad = expected_samples - audio.shape[1]
    audio = np.pad(audio, ((0, 0), (0, pad)), mode="constant")
audio = audio[:, :expected_samples]

input_tensor = audio[np.newaxis, ...].astype(np.float32)

# Warm-up
for _ in range(3):
    _ = session.run([output_name], {input_name: input_tensor})

outputs = session.run([output_name], {input_name: input_tensor})
separated = outputs[0]  # (1, 4, 2, N)

# Extract "other" stem (index 3)
speech = separated[0, 3, :, :]  # (2, N)

sf.write(output_path, speech.T, TARGET_SR, subtype="FLOAT")
print(f"Saved speech to {output_path}")
