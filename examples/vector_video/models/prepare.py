"""Reproduce the two batch-one graphs embedded by the Rust publisher."""
import hashlib
import sys
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import onnxsim


def specialize(source, dest, size, digest):
    """Verify the original graph, adapt its shapes and padding, and check both outputs."""
    if hashlib.sha256(source.read_bytes()).hexdigest() != digest:
        raise ValueError(f"Unexpected SHA-256 for {source}")
    original = onnx.load(source)
    name = original.graph.input[0].name
    shape = [1, 3, size, size]
    model, valid = onnxsim.simplify(original, overwrite_input_shapes={name: shape})
    if not valid:
        raise ValueError(f"Could not simplify {source}")
    needs_zero = False
    for node in model.graph.node:
        if node.op_type == "Pad":
            if len(node.input) == 2:
                node.input.append("")
            if not node.input[2]:
                node.input[2] = "padding_zero"
                needs_zero = True
    if needs_zero:
        model.graph.initializer.append(onnx.numpy_helper.from_array(np.float32(0), "padding_zero"))
    onnx.checker.check_model(model)

    sample = {name: np.random.default_rng(125).random(shape, dtype=np.float32)}
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1

    def evaluate(graph):
        return ort.InferenceSession(
            graph.SerializeToString(), options, providers=["CPUExecutionProvider"]
        ).run(None, sample)

    for before, after in zip(evaluate(original), evaluate(model), strict=True):
        np.testing.assert_allclose(before, after, rtol=1e-4, atol=5e-4)
    onnx.save(model, dest)


if __name__ == "__main__":
    inputs = Path(sys.argv[1])
    output = Path(__file__).resolve().parent
    specialize(inputs / "face_detection_short_range.onnx", output / "blazeface.onnx", 128,
               "2f2689b040becf555706d2cb978d2f0e3296ea82413734fba9a856c66c5f2b17")
    specialize(inputs / "face_landmarker_Nx3x256x256.onnx", output / "face-landmarker.onnx", 256,
               "111795f8703cdeb6d0c68a9f3cc966a0f23f8786bb00f4577a11f461fc4276ac")
