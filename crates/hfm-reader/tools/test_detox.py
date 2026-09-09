# /// script
# requires-python = ">=3.8"
# dependencies = [
#     "transformers",
#     "torch",
#     "accelerate",
# ]
# ///

from transformers import BartForConditionalGeneration, AutoTokenizer
import re

# === LOAD FROM LOCAL DIRECTORY ===
model_path = './models/bart-base-detox'  # Relative to your project root

tokenizer = AutoTokenizer.from_pretrained(model_path)
model = BartForConditionalGeneration.from_pretrained(model_path)

def detoxify(text, max_input_tokens=512, max_output_new_tokens=512):
    # === INPUT HANDLING ===
    # Option 1: Truncate silently if input exceeds 512
    input_ids = tokenizer.encode(
        text,
        return_tensors='pt',
        truncation=True,
        max_length=max_input_tokens
    )
    
    # Option 2: (Uncomment) Raise an error if input exceeds 512 instead of truncating
    # raw_ids = tokenizer.encode(text, truncation=False)
    # if len(raw_ids) > max_input_tokens:
    #     raise ValueError(f"Input exceeds {max_input_tokens} tokens. Got {len(raw_ids)} tokens.")
    # input_ids = torch.tensor([raw_ids])  # Convert back to tensor
    
    # === OUTPUT GENERATION ===
    # Strictly cap the generated output to 512 new tokens
    output_ids = model.generate(
        input_ids,
        max_new_tokens=max_output_new_tokens,
        num_return_sequences=1,
        early_stopping=True
    )

    decoded = tokenizer.decode(output_ids[0], skip_special_tokens=True)
    
    # Optional: warn if output was cut off
    if len(output_ids[0]) >= max_output_new_tokens:
        print(f"⚠️  Warning: Output hit the {max_output_new_tokens}-token limit.")

    return decoded

# Read and parse the file
with open('./tools/text_has_profanity.txt', 'r', encoding='utf-8') as f:
    content = f.read()

# Extract all <p> blocks
blocks = re.findall(r'<p>(.*?)</p>', content, re.DOTALL)

if not blocks:
    print("No <p> tags found.")
    exit()

print("=" * 60)
print(f"PROCESSING {len(blocks)} BLOCKS (sentences 1–20, then paragraph)")
print("=" * 60)

for idx, block in enumerate(blocks, 1):
    cleaned = block.strip()
    if not cleaned:
        continue

    result = detoxify(cleaned)

    print(f"\n--- Block {idx} ---")
    print(f"Input:  {cleaned}")
    print(f"Output: {result}")