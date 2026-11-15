# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "transformers",
#   "torch",
#   "sentencepiece",
#   "protobuf",
# ]
# ///

from pathlib import Path
from transformers import pipeline

labels = ["profanity", "religious", "other"]
# note: these were temporary file paths
input_file = Path("words.txt")
output_file = Path("sorted.txt")

words = [line.strip() for line in input_file.read_text(encoding="utf-8").splitlines() if line.strip()]

print(f"Loaded {len(words)} words. Loading multilingual classifier...")
classifier = pipeline(
    "zero-shot-classification",
    model="MoritzLaurer/mDeBERTa-v3-base-xnli-multilingual-nli-2mil7",
)

buckets = {lbl: [] for lbl in labels}

# Batching all words at once is much faster than looping one by one
results = classifier(words, candidate_labels=labels, batch_size=16)

for word, res in zip(words, results):
    top_label = res["labels"][0]
    buckets[top_label].append(word)

formatted = []
for cat, items in buckets.items():
    if items:
        formatted.append(f"# {cat}\n" + "\n".join(items))

output_file.write_text("\n\n".join(formatted) + "\n", encoding="utf-8")
print(f"Done! Saved categorized list to {output_file}")
