"""One picture through one ONNX image tagger; prints the tags as JSON and exits.

Run by DEAS (src-tauri/src/analysis.rs) inside its private analysis runtime, never
inside ComfyUI's environment. Input contracts, told apart by the model's own
input shape:

- WD (SmilingWolf wd-*-tagger-v3, wd-v1-4-*, and PromptForge's animetimm export):
  NHWC float32, BGR, 0-255, the picture padded to a white square first.
- timm (animetimm's own ONNX files): NCHW float32, RGB, 0-1 then normalised, with the
  steps read from preprocess.json beside the model. Logits get a sigmoid.
- Other NCHW taggers without a preprocess.json (Camie, PixAI, JoyTag, cl_tagger, ...):
  padded to a white square, resized to the model's input size, RGB 0-1, normalised with
  the mean/std from a config.json beside the model if it names them, else 0.5/0.5.

Labels arrive from DEAS already read, whatever shape the tagger shipped them in: a JSON
list of [tag, kind] in output order, kind g(eneral), c(haracter), r(ating) or o(ther).
The output used is the one with one score per tag. Only tags at or above the
thresholds come back, highest first.
"""

import argparse
import json
import os
import sys

import numpy as np
import onnxruntime as ort
from PIL import Image

RATING, GENERAL, CHARACTER = "r", "g", "c"


def labels(path):
    with open(path, encoding="utf-8") as f:
        return [(name, kind) for name, kind in json.load(f)]


def side_config(model):
    """mean/std from a config.json beside the model (timm's pretrained_cfg or flat)."""
    path = os.path.join(os.path.dirname(model), "config.json")
    try:
        with open(path, encoding="utf-8") as f:
            cfg = json.load(f)
    except (OSError, ValueError):
        return None
    for c in (cfg.get("pretrained_cfg") or {}, cfg):
        if isinstance(c, dict) and isinstance(c.get("mean"), list) and isinstance(c.get("std"), list):
            return c["mean"], c["std"]
    return None


def plain_nchw(image, height, width, norm):
    mean, std = norm or ([0.5, 0.5, 0.5], [0.5, 0.5, 0.5])
    array = np.asarray(pad_square(image).resize((width, height), Image.BICUBIC), dtype=np.float32) / 255.0
    array = (array - np.asarray(mean, dtype=np.float32)) / np.asarray(std, dtype=np.float32)
    return np.ascontiguousarray(array.transpose(2, 0, 1)[np.newaxis].astype(np.float32))


def flatten(image):
    """RGB on white: transparent pixels are white, as the taggers were trained."""
    image = image.convert("RGBA") if image.mode in ("RGBA", "LA", "P") else image.convert("RGB")
    if image.mode == "RGBA":
        canvas = Image.new("RGBA", image.size, (255, 255, 255, 255))
        canvas.alpha_composite(image)
        image = canvas
    return image.convert("RGB")


def pad_square(image, size=None):
    side = max(image.size)
    canvas = Image.new("RGB", (side, side), (255, 255, 255))
    canvas.paste(image, ((side - image.size[0]) // 2, (side - image.size[1]) // 2))
    return canvas if size is None else canvas.resize((size, size), Image.BICUBIC)


def wd_input(image, size):
    array = np.asarray(pad_square(image, size), dtype=np.float32)[:, :, ::-1]  # RGB -> BGR
    return np.ascontiguousarray(array[np.newaxis])


def timm_input(image, steps):
    for step in steps:
        kind = step.get("type")
        if kind == "pad_to_size":
            w, h = step["size"]
            ratio = min(w / image.size[0], h / image.size[1])
            inner = image.resize((max(1, round(image.size[0] * ratio)), max(1, round(image.size[1] * ratio))), Image.BILINEAR)
            canvas = Image.new("RGB", (w, h), (255, 255, 255))
            canvas.paste(inner, ((w - inner.size[0]) // 2, (h - inner.size[1]) // 2))
            image = canvas
        elif kind == "resize":
            size = step["size"]
            if isinstance(size, int):
                short = min(image.size)
                scale = size / short
                image = image.resize((round(image.size[0] * scale), round(image.size[1] * scale)), Image.BICUBIC)
            else:
                image = image.resize(tuple(size), Image.BICUBIC)
        elif kind == "center_crop":
            w, h = step["size"]
            left, top = (image.size[0] - w) // 2, (image.size[1] - h) // 2
            image = image.crop((left, top, left + w, top + h))
        elif kind == "maybe_to_tensor":
            image = np.asarray(image, dtype=np.float32).transpose(2, 0, 1) / 255.0
        elif kind == "normalize":
            mean = np.asarray(step["mean"], dtype=np.float32)[:, None, None]
            std = np.asarray(step["std"], dtype=np.float32)[:, None, None]
            image = (image - mean) / std
    if not isinstance(image, np.ndarray):
        raise SystemExit("preprocess.json never turned the picture into a tensor")
    return np.ascontiguousarray(image[np.newaxis].astype(np.float32))


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--model", required=True)
    p.add_argument("--labels", required=True)
    p.add_argument("--image", required=True)
    p.add_argument("--general", type=float, default=0.35)
    p.add_argument("--character", type=float, default=0.85)
    p.add_argument("--threads", type=int, default=0)
    a = p.parse_args()

    options = ort.SessionOptions()
    if a.threads > 0:
        options.intra_op_num_threads = a.threads
    session = ort.InferenceSession(a.model, sess_options=options, providers=["CPUExecutionProvider"])
    first = session.get_inputs()[0]
    shape = first.shape
    try:
        image = flatten(Image.open(a.image))
    except (OSError, ValueError):
        raise SystemExit("That file is not a picture the tagger can read")
    if len(shape) == 4 and shape[-1] == 3:
        size = shape[1] if isinstance(shape[1], int) else 448
        tensor = wd_input(image, size)
        layout = "wd"
    elif len(shape) == 4 and shape[1] == 3:
        pre = os.path.join(os.path.dirname(a.model), "preprocess.json")
        if os.path.isfile(pre):
            with open(pre, encoding="utf-8") as f:
                steps = json.load(f).get("test", [])
            tensor = timm_input(image, steps)
            layout = "timm"
        else:
            h = shape[2] if isinstance(shape[2], int) else 448
            w = shape[3] if isinstance(shape[3], int) else h
            tensor = plain_nchw(image, h, w, side_config(a.model))
            layout = "nchw"
    else:
        raise SystemExit(f"Unrecognised tagger input shape {shape}")

    names = labels(a.labels)
    outputs = session.run(None, {first.name: tensor})
    # The output with one score per tag; a model with several (Camie's initial and
    # refined, PixAI's embedding) is read from its last such output.
    fitting = [o for o in outputs if getattr(o, "ndim", 0) >= 1 and o.shape[-1] == len(names)]
    if not fitting:
        sizes = ", ".join(str(o.shape[-1]) for o in outputs if getattr(o, "ndim", 0) >= 1)
        raise SystemExit(f"The tag list has {len(names)} tags but the model scores {sizes}; the list beside it is for a different model")
    scores = fitting[-1].reshape(-1, len(names))[0].astype(np.float64)
    if scores.min() < 0 or scores.max() > 1:
        scores = 1.0 / (1.0 + np.exp(-scores))

    out = {"layout": layout, "rating": {}, "general": [], "character": []}
    for (name, category), score in zip(names, scores):
        score = float(score)
        if category == RATING:
            out["rating"][name] = round(score, 4)
        elif category == CHARACTER and score >= a.character:
            out["character"].append([name, round(score, 4)])
        elif category == GENERAL and score >= a.general:
            out["general"].append([name, round(score, 4)])
    for key in ("general", "character"):
        out[key].sort(key=lambda t: -t[1])
    json.dump(out, sys.stdout)


if __name__ == "__main__":
    main()
