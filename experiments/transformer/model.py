"""Temporal and candidate-token ranker over the experiment's fixed features."""
import torch
from torch import nn


class CandidateTransformer(nn.Module):
    def __init__(self, features: int, history: int = 8, width: int = 64, layers: int = 2):
        super().__init__()
        self.history = history
        self.input = nn.Linear(features, width)
        self.history_position = nn.Parameter(torch.randn(history, width) * 0.02)
        self.token_type = nn.Embedding(2, width)
        layer = nn.TransformerEncoderLayer(
            d_model=width, nhead=4, dim_feedforward=width * 2,
            dropout=0.1, batch_first=True, activation="gelu",
        )
        self.encoder = nn.TransformerEncoder(layer, num_layers=layers, enable_nested_tensor=False)
        # Encoder layers are cloned by PyTorch; initialize their matrices independently.
        for block in self.encoder.layers:
            for parameter in block.parameters():
                if parameter.dim() > 1:
                    nn.init.xavier_uniform_(parameter)
        self.output = nn.Linear(width, 1)

    def forward(self, history, candidates, history_mask, candidate_mask):
        past = self.input(history) + self.history_position + self.token_type.weight[0]
        current = self.input(candidates) + self.token_type.weight[1]
        tokens = torch.cat([past, current], dim=1)
        padding = ~torch.cat([history_mask, candidate_mask], dim=1)
        encoded = self.encoder(tokens, src_key_padding_mask=padding)
        scores = self.output(encoded[:, self.history:]).squeeze(-1)
        return scores.masked_fill(~candidate_mask, -1e9)


def pairwise_loss(scores, labels, valid):
    pairs = (labels[:, :, None] > labels[:, None, :]) & valid[:, :, None] & valid[:, None, :]
    count = pairs.sum(dim=(1, 2))
    losses = torch.nn.functional.softplus(-(scores[:, :, None] - scores[:, None, :]))
    per_group = (losses * pairs).sum(dim=(1, 2)) / count.clamp_min(1)
    informative = count > 0
    return per_group[informative].mean() if informative.any() else scores.sum() * 0
