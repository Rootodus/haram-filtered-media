import torch
import torch.nn as nn


class DummyModel(nn.Module):
    def __init__(self):
        super().__init__()
        self.linear = nn.Linear(410, 2)  # input 410, output 2 actions

    def forward(self, x):
        return self.linear(x)


# Create model with random weights
model = DummyModel()
model.eval()

# Dummy input for tracing
dummy_input = torch.randn(256, 410)

# Export to ONNX
torch.onnx.export(
    model,
    dummy_input,
    "dummy_model.onnx",
    input_names=["input"],
    output_names=["output"],
    dynamic_axes=None,  # fixed shapes
    opset_version=11,
)

print("Generated dummy_model.onnx with input [256,410] output [256,2]")
