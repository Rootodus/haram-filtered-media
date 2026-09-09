from transformers import BartForConditionalGeneration, AutoTokenizer

# Load the base tokenizer and the fine-tuned model
base_model_name = 'facebook/bart-base'
model_name = 's-nlp/bart-base-detox'

tokenizer = AutoTokenizer.from_pretrained(base_model_name)
model = BartForConditionalGeneration.from_pretrained(model_name)

def detoxify(text):
    input_ids = tokenizer.encode(text, return_tensors='pt')
    output_ids = model.generate(
        input_ids,
        max_length=50,
        num_return_sequences=1,
        early_stopping=True
    )
    return tokenizer.decode(output_ids[0], skip_special_tokens=True)

# Test with a single sentence
print(detoxify("This is completely idiotic!"))
# Expected output: "This is unwise!"[reference:1][reference:2]