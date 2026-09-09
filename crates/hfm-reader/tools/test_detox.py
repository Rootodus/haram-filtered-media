from transformers import BartForConditionalGeneration, AutoTokenizer
import re

# Load model and tokenizer
base_model_name = 'facebook/bart-base'
model_name = 's-nlp/bart-base-detox'

tokenizer = AutoTokenizer.from_pretrained(base_model_name)
model = BartForConditionalGeneration.from_pretrained(model_name)

def detoxify(text, max_length=512):
    # Encode with truncation to stay within BART's 1024-token limit
    input_ids = tokenizer.encode(
        text,
        return_tensors='pt',
        truncation=True,
        max_length=1024
    )
    output_ids = model.generate(
        input_ids,
        max_length=max_length,          # Cap for output (works for both short & long)
        num_return_sequences=1,
        early_stopping=True             # Stops early if EOS is generated
    )
    return tokenizer.decode(output_ids[0], skip_special_tokens=True)

# Read and parse the file
with open('input.txt', 'r', encoding='utf-8') as f:
    content = f.read()

# Extract all text inside <p> tags
blocks = re.findall(r'<p>(.*?)</p>', content, re.DOTALL)

if not blocks:
    print("No <p> tags found in input.txt")
    exit()

# Process every block (20 sentences + 1 paragraph) identically
print("=" * 60)
print(f"PROCESSING {len(blocks)} BLOCKS (sentences 1–20, then paragraph)")
print("=" * 60)

for idx, block in enumerate(blocks, 1):
    cleaned = block.strip()
    if not cleaned:
        continue
    
    result = detoxify(cleaned, max_length=512)  # Same cap for all
    
    print(f"\n--- Block {idx} ---")
    print(f"Input:  {cleaned}")
    print(f"Output: {result}")