import numpy as np
import librosa
import onnxruntime as ort
import soundfile as sf

# Parameters (from metadata and Spleeter defaults)
SAMPLE_RATE = 41000
N_FFT = 1024
HOP_LENGTH = 512
WIN_LENGTH = 1024
WINDOW = "hann"
EXPECTED_FRAMES = 1024
EXPECTED_BINS = 512

# Load your audio file (replace with your file path)
audio_path = "../assets/mixed_audio.wav"  # must be stereo, sample rate can be anything; we'll resample
audio, sr = librosa.load(audio_path, sr=SAMPLE_RATE, mono=False)  # stereo, shape (2, N)
if audio.ndim == 1:
    audio = np.stack([audio, audio])  # convert mono to stereo by duplication

# Ensure we have enough samples; if not, pad with zeros.
total_samples = (EXPECTED_FRAMES - 1) * HOP_LENGTH
if audio.shape[1] < total_samples:
    pad_len = total_samples - audio.shape[1]
    audio = np.pad(audio, ((0, 0), (0, pad_len)), mode="constant")

# Take the first total_samples samples (or we could take a random segment)
audio = audio[:, :total_samples]

# Compute STFT for each channel
stft_list = []
for ch in range(2):
    stft = librosa.stft(
        audio[ch],
        n_fft=N_FFT,
        hop_length=HOP_LENGTH,
        win_length=WIN_LENGTH,
        window=WINDOW,
        center=True,
    )
    stft_list.append(stft)
# stft shape for each: (513, 1024) because we have exactly 1024 frames

# Convert to magnitude and reshape to (channels, freq, time)
mag = np.array([np.abs(stft) for stft in stft_list])  # (2, 513, 1024)

# Drop DC bin (index 0) to get 512 frequency bins
mag = mag[:, 1:, :]  # (2, 512, 1024)

# Add batch dimension -> (2, 1, 512, 1024)
input_tensor = np.expand_dims(mag, axis=1).astype(np.float32)

# Load ONNX model
session = ort.InferenceSession(
    "../models/sherpa-onnx-spleeter-2stems-fp16/vocals.fp16.onnx",
    providers=["CPUExecutionProvider"],  # change to Dml if installed
)

# Run inference
outputs = session.run(None, {session.get_inputs()[0].name: input_tensor})
separated_mag = outputs[0]  # shape (2, 1, 512, 1024) -> channels, batch, freq, time
separated_mag = separated_mag[:, 0, :, :]  # remove batch -> (2, 512, 1024)

# Reconstruct phase from original mixture (reuse input phase)
# We need to add back the DC bin (zeros) to match original freq bins.
phase = np.array([np.angle(stft) for stft in stft_list])  # (2, 513, 1024)
phase_dc = phase[:, 0:1, :]  # keep DC
phase_rest = phase[:, 1:, :]  # rest

# For the separated vocals, we will use the separated magnitude and the original phase.
# But we must match the original frequency bin count: we have 512 bins (excluding DC) but original had 513.
# We'll reconstruct by placing the separated magnitude into the non-DC bins and keeping DC as zero (or original DC).
separated_complex = np.zeros((2, 513, 1024), dtype=np.complex64)
separated_complex[:, 1:, :] = separated_mag * np.exp(
    1j * phase_rest
)  # reuse original phase for non-DC

# Inverse STFT for each channel
reconstructed = []
for ch in range(2):
    audio_out = librosa.istft(
        separated_complex[ch],
        hop_length=HOP_LENGTH,
        win_length=WIN_LENGTH,
        window=WINDOW,
        center=True,
    )
    reconstructed.append(audio_out)

# Convert to stereo interleaved (or separate files)
reconstructed = np.array(reconstructed)  # (2, N)

# Save as WAV
sf.write("../assets/vocals_output.wav", reconstructed.T, SAMPLE_RATE, subtype="FLOAT")
print("Saved output.")
