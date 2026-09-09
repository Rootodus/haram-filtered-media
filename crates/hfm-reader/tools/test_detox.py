from transformers import BartForConditionalGeneration, AutoTokenizer
import re

# Load model and tokenizer
base_model_name = 'facebook/bart-base'
model_name = 's-nlp/bart-base-detox'

tokenizer = AutoTokenizer.from_pretrained(base_model_name)
model = BartForConditionalGeneration.from_pretrained(model_name)

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
        max_new_tokens=max_output_new_tokens,  # This is the hard cap for output
        num_return_sequences=1,
        early_stopping=True
    )
    
    decoded = tokenizer.decode(output_ids[0], skip_special_tokens=True)
    
    # (Optional) Check if the output was cut off by the 512 limit
    output_token_count = len(output_ids[0])
    if output_token_count >= max_output_new_tokens:
        print(f"⚠️  Warning: Output hit the 512-token limit and was truncated.")
    
    return decoded

# Read and parse the file
with open('text_has_profanity.txt', 'r', encoding='utf-8') as f:
    content = f.read()

# Extract all text inside <p> tags
blocks = re.findall(r'<p>(.*?)</p>', content, re.DOTALL)

if not blocks:
    print("No <p> tags found in input.txt")
    exit()

print("=" * 60)
print(f"PROCESSING {len(blocks)} BLOCKS (sentences 1–20, then paragraph)")
print("=" * 60)

for idx, block in enumerate(blocks, 1):
    cleaned = block.strip()
    if not cleaned:
        continue
    
    # Both input and output are strictly capped at 512
    result = detoxify(cleaned, max_input_tokens=512, max_output_new_tokens=512)
    
    print(f"\n--- Block {idx} ---")
    print(f"Input:  {cleaned}")
    print(f"Output: {result}")