# Layered polygon models

## Face guidance

The example embeds two batch-one CPU inference graphs. BlazeFace finds candidate faces. Face Landmarker predicts 478 points for each accepted crop, including four boundary points per iris. The encoder selects nine polygon outlines from those points. [Google MediaPipe](https://github.com/google-ai-edge/mediapipe) · [ONNX exports and attribution](https://github.com/yakhyo/mediapipe-face-mesh-onnx) · [Apache 2.0 license](LICENSE)

| Local graph | Upstream release asset | Original SHA-256 |
| --- | --- | --- |
| blazeface.onnx | [face_detection_short_range.onnx](https://github.com/yakhyo/mediapipe-face-mesh-onnx/releases/download/weights/face_detection_short_range.onnx) | `2f2689b040becf555706d2cb978d2f0e3296ea82413734fba9a856c66c5f2b17` |
| face-landmarker.onnx | [face_landmarker_Nx3x256x256.onnx](https://github.com/yakhyo/mediapipe-face-mesh-onnx/releases/download/weights/face_landmarker_Nx3x256x256.onnx) | `111795f8703cdeb6d0c68a9f3cc966a0f23f8786bb00f4577a11f461fc4276ac` |

The bundled copies specialize the input to a single crop, fold constant graph operations, and supply the ONNX default zero value explicitly to padding operations. To regenerate, put the two original release assets in a directory and run the following command from this folder. It checks their hashes and compares both original and adapted graph outputs on a seeded input.

```sh
uv run --python 3.13 --with onnx==1.23.2 --with onnxsim==0.8.1 --with onnxruntime==1.32.0 prepare.py /path/to/originals
```

## Object guidance

The object guidance model is U²-NetP by Xuebin Qin, Zichen Zhang, Chenyang Huang, Masood Dehghan, Osmar R. Zaïane, and Martin Jägersand. The example embeds an ONNX export specialized to one 160×160 RGB image and its fused foreground mask. [Original model and source](https://github.com/xuebinqin/U-2-Net) · [Apache 2.0 license](LICENSE) · [Upstream ONNX export](https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx)

Run the following from this directory to regenerate the bundled `u2netp.onnx`. The script checks the original artifact hash and compares the specialized graph with the simplified graph on a seeded input.

```sh
uv run --python 3.13 --with onnx==1.23.2 --with onnxsim==0.8.1 --with onnxruntime==1.32.0 prepare_objects.py /path/to/original/u2netp.onnx
```

## Part guidance

The experimental `--part-model` option loads an external ONNX graph derived from DINOv2 Small. It compares the source with the broad polygon fills and ranks regions where detail can improve the reconstruction. Generate the graph from a local copy of the published checkpoint containing `config.json` and `model.safetensors`. The graph accepts a normalized `[2, 3, 126, 224]` source and draft pair and returns a `[9, 16]` float loss grid. It can also return a second `[1, 1, 9, 16]` float grid of foreground probabilities from zero to one. [DINOv2 Small checkpoint and license](https://huggingface.co/facebook/dinov2-small)

```sh
uv run --python 3.13 --with torch==2.7.1 --with transformers==4.53.1 --with onnx==1.23.2 --with onnxruntime==1.32.0 \
  prepare_parts.py /path/to/dinov2-small /path/to/parts.onnx
```

The default export uses eight transformer blocks. Use `--blocks 12` to export the full comparison model.

Add `--with onnxsim==0.8.1` to `uv run` and `--simplify` to the script arguments to fold fixed shapes and constants. On the eight-block export with foreground guidance, this reduced the graph from 852 to 326 ONNX nodes. Creating a Core ML session and running its first inference took 8.7 seconds with the simplified export and 10.4 seconds with the original on an Apple M4 Pro.

Use `--foreground-head /path/to/head.pt` to add foreground guidance with a trained `ForegroundHead` state dictionary. Train the head against foreground coverage per 14×14 source patch with the backbone frozen. The exporter concatenates three normalized descriptors for each patch. These are the final block output, the fourth block output, and the final block output with the image's mean patch descriptor subtracted. Match the backbone depth used for training when exporting.
