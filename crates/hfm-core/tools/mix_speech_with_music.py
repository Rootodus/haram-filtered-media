import librosa
import soundfile as sf
import numpy as np

# Paths to your source files
speech_path = "../assets/speech.wav"  # speech-only audio
music_path = "../assets/music_bg.wav"  # music-only audio (instrumental)
output_path = "../assets/mixed_audio.wav"

# Target sample rate (must match Spleeter's model, i.e., 41000 Hz)
TARGET_SR = 41000

# Load and resample
speech, sr_speech = librosa.load(speech_path, sr=TARGET_SR, mono=False)
music, sr_music = librosa.load(music_path, sr=TARGET_SR, mono=False)

# Ensure stereo (if mono, duplicate to stereo)
if speech.ndim == 1:
    speech = np.stack([speech, speech])
if music.ndim == 1:
    music = np.stack([music, music])

# Trim/pad to the same length (use the longer of the two)
min_len = min(speech.shape[1], music.shape[1])
speech = speech[:, :min_len]
music = music[:, :min_len]

# Adjust gain: music should be about -12 dB to -18 dB relative to speech
# (i.e., music_volume = speech_volume * 0.25 ~ 0.1)
music_scale = 0.15  # adjust to your taste
music = music * music_scale

# Mix
mixed = speech + music

# Clip to avoid distortion
mixed = np.clip(mixed, -1.0, 1.0)

# Save as WAV
sf.write(output_path, mixed.T, TARGET_SR, subtype="FLOAT")
print(f"Mixed audio saved to {output_path}")
