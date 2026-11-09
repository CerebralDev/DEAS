"""
Turns an animetimm checkpoint (timm / PyTorch) into an ONNX tagger the Media Analysis
worker runs like a WD tagger. Ported from PromptForge's scripts/export-animetimm-onnx.py.

DEAS runs this once per checkpoint, in a throwaway environment holding torch, timm,
onnx and onnxruntime, and removes the environment afterwards:

  python animetimm_export.py --src <animetimm folder> --out <empty folder>

It writes model.onnx, model.onnx.data and selected_tags.csv into --out, and nothing
anywhere else: the source folder is only read.

Why a wrapper rather than a plain timm export: the worker feeds WD-shaped input
(NHWC float32, BGR, 0-255, padded to a white square) and reads the output as
probabilities. A timm model wants NCHW, RGB, normalised input and returns logits. So
the channel flip, transpose, scaling, normalisation (from the checkpoint's own
preprocess.json) and sigmoid all live inside the exported graph.

Each stage prints one line starting with "STAGE " so DEAS can show where it is; any
failure exits non-zero with one plain line saying what to do.
"""

import argparse
import json
import shutil
import sys
from pathlib import Path


def fail(message):
    print(message, file=sys.stderr, flush=True)
    sys.exit(1)


def stage(name):
    print(f"STAGE {name}", flush=True)


parser = argparse.ArgumentParser()
parser.add_argument("--src", required=True)
parser.add_argument("--out", required=True)
args = parser.parse_args()
SRC = Path(args.src)
OUT_DIR = Path(args.out)

for need in ("config.json", "preprocess.json", "selected_tags.csv"):
    if not (SRC / need).is_file():
        fail(f"{need} is missing from the tagger's folder, so it cannot be converted.")
weights = next((SRC / w for w in ("model.safetensors", "pytorch_model.bin") if (SRC / w).is_file()), None)
if weights is None:
    fail("The tagger's folder has no model.safetensors or pytorch_model.bin to convert.")

# The copy is about the size of the weights (both float32); leave room for the
# scratch export beside it and a margin.
OUT_DIR.mkdir(parents=True, exist_ok=True)
need = weights.stat().st_size * 2.1 + 512 * 1024 * 1024
free = shutil.disk_usage(OUT_DIR).free
if free < need:
    fail(f"Not enough free space: the conversion needs about {need / 1e9:.1f} GB on the drive DEAS keeps its "
         f"files on and {free / 1e9:.1f} GB is free. Free some space, then convert again.")

stage("loading")
import torch
import torch.nn as nn
import timm

cfg = json.loads((SRC / "config.json").read_text(encoding="utf-8"))
pre = json.loads((SRC / "preprocess.json").read_text(encoding="utf-8"))
if "architecture" not in cfg or "num_classes" not in cfg:
    fail("config.json does not name a timm architecture, so this is not an animetimm checkpoint DEAS can convert.")
try:
    norm = next(s for s in pre["test"] if s["type"] == "normalize")
    size = next(s for s in pre["test"] if s["type"] == "center_crop")["size"][0]
except (KeyError, StopIteration):
    fail("preprocess.json has no test-time normalize and center_crop steps, so the input cannot be set up.")

try:
    model = timm.create_model(cfg["architecture"], pretrained=False, num_classes=cfg["num_classes"], **cfg.get("model_args", {}))
except Exception as e:  # an architecture this timm does not know
    fail(f"timm cannot build {cfg['architecture']} ({e}).")

if weights.suffix == ".safetensors":
    from safetensors.torch import load_file
    state = load_file(str(weights), device="cpu")
else:
    state = torch.load(weights, map_location="cpu", weights_only=True)
missing, unexpected = model.load_state_dict(state, strict=False)
del state
if any("head" in n for n in missing):
    # The wrong architecture or class count: it would run and return nonsense.
    fail("The checkpoint's classifier did not load, so the conversion was stopped.")
model.eval()


class WdCompatible(nn.Module):
    """Presents a timm tagger through the interface WD ONNX files use."""

    def __init__(self, inner, mean, std):
        super().__init__()
        self.inner = inner
        self.register_buffer("mean", torch.tensor(mean).view(1, 3, 1, 1))
        self.register_buffer("std", torch.tensor(std).view(1, 3, 1, 1))

    def forward(self, x):
        # x arrives as the worker builds it for WD: [N, H, W, 3], BGR, 0-255.
        x = x.flip(-1)
        x = x.permute(0, 3, 1, 2)
        x = x / 255.0
        x = (x - self.mean) / self.std
        return torch.sigmoid(self.inner(x))


wrapped = WdCompatible(model, norm["mean"], norm["std"]).eval()
dummy = torch.rand(1, size, size, 3) * 255.0
with torch.no_grad():
    ref = wrapped(dummy)
if ref.shape[1] != cfg["num_classes"]:
    fail(f"The model returned {ref.shape[1]} scores for {cfg['num_classes']} tags, so the conversion was stopped.")

stage("exporting")
OUT = OUT_DIR / "model.onnx"
DATA = "model.onnx.data"
TMP_DIR = OUT_DIR / "_export"
if TMP_DIR.exists():
    shutil.rmtree(TMP_DIR)
TMP_DIR.mkdir()
torch.onnx.export(
    wrapped,
    dummy,
    str(TMP_DIR / "model.onnx"),
    input_names=["input"],
    output_names=["output"],
    dynamic_axes={"input": {0: "batch"}, "output": {0: "batch"}},
    opset_version=17,
    dynamo=False,
)
del model, wrapped

import onnx

# torch writes tensors over 2 GB one file per tensor; re-save as one graph and one data file.
loaded = onnx.load(str(TMP_DIR / "model.onnx"))
onnx.save_model(loaded, str(OUT), save_as_external_data=True, all_tensors_to_one_file=True, location=DATA, size_threshold=0)
del loaded
shutil.rmtree(TMP_DIR)

stage("checking")
# An export that runs but disagrees with torch would surface months later as "the tags got worse".
import numpy as np
import onnxruntime as ort

sess = ort.InferenceSession(str(OUT), providers=["CPUExecutionProvider"])
meta = sess.get_inputs()[0]
got = sess.run(None, {meta.name: dummy.numpy()})[0]
delta = float(np.abs(got - ref.numpy()).max())
if delta > 1e-3:
    fail(f"The converted tagger disagrees with the original (by {delta:.1e}), so it was not kept.")
if meta.shape[1] != size:
    fail("The converted tagger has the wrong input layout, so it was not kept.")
del sess

# The tag list is the checkpoint's own, byte for byte, copied only once the check passed.
shutil.copyfile(SRC / "selected_tags.csv", OUT_DIR / "selected_tags.csv")
print(f"OK {delta:.2e}", flush=True)
