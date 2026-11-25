"""Export DINOv2 detail and optional foreground guidance for layered polygons."""

import argparse
from pathlib import Path

import numpy as np
import onnxruntime as ort
import torch
from transformers import Dinov2Model


class PartLoss(torch.nn.Module):
    """Compare normalized patch descriptors of the source and draft in one batch."""

    def __init__(self, backbone: Dinov2Model, head: torch.nn.Module | None) -> None:
        super().__init__()
        self.backbone = backbone
        self.head = head

    def forward(self, pair: torch.Tensor) -> tuple[torch.Tensor, ...]:
        states = self.backbone(pair, output_hidden_states=True).hidden_states
        features = states[-1][:, 1:]
        source, draft = torch.nn.functional.normalize(features, dim=-1).unbind()
        loss = (1 - (source * draft).sum(dim=-1)).clamp_min(0).reshape(9, 16)
        if self.head is None:
            return (loss,)
        local = torch.nn.functional.layer_norm(features[:1], (384,))
        early = torch.nn.functional.layer_norm(states[4][:1, 1:], (384,))
        centered = torch.nn.functional.layer_norm(local - local.mean(dim=1, keepdim=True), (384,))
        foreground = self.head(torch.cat((local, early, centered), dim=-1))
        return loss, foreground.sigmoid().reshape(1, 1, 9, 16)


class ForegroundHead(torch.nn.Module):
    """Predict a foreground probability from each source patch descriptor."""

    def __init__(self) -> None:
        super().__init__()
        self.layers = torch.nn.Sequential(
            torch.nn.Linear(1152, 128), torch.nn.GELU(), torch.nn.Dropout(0.12), torch.nn.Linear(128, 1)
        )

    def forward(self, features: torch.Tensor) -> torch.Tensor:
        return self.layers(features)


def main() -> None:
    """Export a fixed-shape graph and verify its output against PyTorch."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkpoint", type=Path, help="Local facebook/dinov2-small checkpoint directory")
    parser.add_argument("output", type=Path, help="Output ONNX model")
    parser.add_argument("--blocks", type=int, choices=(8, 12), default=8)
    parser.add_argument("--foreground-head", type=Path, help="Optional trained ForegroundHead state dictionary")
    parser.add_argument("--simplify", action="store_true", help="Fold fixed shapes and constants with onnxsim")
    args = parser.parse_args()

    torch.set_num_threads(1)
    torch.manual_seed(23)
    backbone = Dinov2Model.from_pretrained(args.checkpoint, local_files_only=True).eval()
    backbone.encoder.layer = torch.nn.ModuleList(list(backbone.encoder.layer)[: args.blocks])
    head = None
    names = ["loss"]
    if args.foreground_head is not None:
        head = ForegroundHead()
        head.load_state_dict(torch.load(args.foreground_head, map_location="cpu", weights_only=True))
        names.append("foreground")
    model = PartLoss(backbone, head).eval()
    pair = torch.rand(2, 3, 126, 224)
    with torch.inference_mode():
        references = model(pair)
        torch.onnx.export(
            model,
            pair,
            args.output,
            input_names=["inputs"],
            output_names=names,
            opset_version=17,
            dynamo=False,
        )
    if args.simplify:
        import onnx
        from onnxsim import simplify

        graph, verified = simplify(
            onnx.load_model(args.output), check_n=1, input_data={"inputs": pair.numpy()}
        )
        if not verified:
            raise ValueError("ONNX simplification changed the part model outputs")
        onnx.save_model(graph, args.output)
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    session = ort.InferenceSession(str(args.output), options, providers=["CPUExecutionProvider"])
    results = session.run(names, {"inputs": pair.numpy()})
    for result, reference in zip(results, references, strict=True):
        np.testing.assert_allclose(result, reference.numpy(), rtol=2e-3, atol=5e-5)


if __name__ == "__main__":
    main()
