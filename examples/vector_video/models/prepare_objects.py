"""Specialize the U²-NetP foreground graph and verify its fused output."""

import argparse
import hashlib
from pathlib import Path

import numpy as np
import onnx
import onnxruntime
import onnxsim

SHA256 = "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8"
SIZE = 160


def main():
    """Adapts a verified upstream artifact into a batch-one publisher model."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("--output", type=Path, default=Path("u2netp.onnx"))
    args = parser.parse_args()
    data = args.original.read_bytes()
    if hashlib.sha256(data).hexdigest() != SHA256:
        raise ValueError("Unexpected U²-NetP source hash")

    model = onnx.load_model_from_string(data)
    image, mask = model.graph.input[0].name, model.graph.output[0].name
    model = onnx.utils.Extractor(model).extract_model([image], [mask])
    for tensor in (model.graph.input[0], model.graph.output[0]):
        tensor.type.tensor_type.shape.dim[2].dim_value = SIZE
        tensor.type.tensor_type.shape.dim[3].dim_value = SIZE
    del model.graph.value_info[:]

    portable, ok = onnxsim.simplify(model, check_n=0)
    if not ok:
        raise ValueError("ONNX simplification failed")
    onnx.checker.check_model(portable)
    opts = onnxruntime.SessionOptions()
    opts.intra_op_num_threads = 1
    pixels = np.random.default_rng(7).normal(size=(1, 3, SIZE, SIZE)).astype(np.float32)

    def infer(graph):
        """Runs the same normalized input through one form of the graph."""
        session = onnxruntime.InferenceSession(
            graph.SerializeToString(), opts, providers=["CPUExecutionProvider"]
        )
        return session.run([mask], {image: pixels})[0]

    np.testing.assert_allclose(infer(portable), infer(model), atol=2e-5, rtol=2e-5)
    onnx.save_model(portable, args.output)


if __name__ == "__main__":
    main()
