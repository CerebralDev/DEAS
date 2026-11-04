"""Owned-backend adapter: clean saver suffixes only for marked DEAS submissions."""
import functools
import os
from pathlib import Path
import re

import folder_paths
import nodes
from server import PromptServer

# `prefix_00001_.png` (image savers), `prefix_00001_audio.webm` and its `-first-frame.png` (video combine).
SAVED = re.compile(r"(.+)_(\d{5,})_?(audio)?(.*)\.(\w+)")
SAVERS = ("SaveImage", "DaSiWa_MetadataImageSaver", "DaSiWa_EnhancedVideoCombine")


def publish(files):
    """Hard-link one save's files to `prefix-00001[-audio]...` at the first counter free for all of them."""
    counter = int(files[0][1].group(2))
    while True:
        done = []
        try:
            for source, match in files:
                prefix, _, audio, rest, extension = match.groups()
                target = source.with_name(f"{prefix}-{counter:05}{'-audio' if audio else ''}{rest}.{extension}")
                # A hard link publishes without overwriting an existing output, on Linux and Windows.
                os.link(source, target)
                done.append(target)
            break
        except BaseException as error:
            rollback_failed = False
            for target in done:
                try:
                    target.unlink()
                except OSError as cleanup_error:
                    error.add_note(f"DEAS output naming rollback failed for {target}: {cleanup_error}")
                    rollback_failed = True
            if not isinstance(error, FileExistsError) or rollback_failed:
                raise
            counter += 1
    for source, _ in files:
        source.unlink()
    return {source.name: target.name for (source, _), target in zip(files, done)}


def wrap_save(save):
    @functools.wraps(save)
    def run(self, *args, **kwargs):
        result = save(self, *args, **kwargs)
        if not (kwargs.get("extra_pnginfo") or {}).get("deas_output_naming"):
            return result
        ui = result.get("ui", {})
        assets = [a for key in ("images", "gifs") for a in ui.get(key, []) if a["type"] == "output"]
        groups = {}
        for asset in assets:
            match = SAVED.fullmatch(asset["filename"])
            if match:
                path = Path(folder_paths.get_output_directory()) / asset["subfolder"] / asset["filename"]
                groups.setdefault((asset["subfolder"], match.group(1), match.group(2)), {})[path] = match
        renamed = {}
        for group in groups.values():
            renamed.update(publish(list(group.items())))
        for asset in assets:
            asset["filename"] = renamed.get(asset["filename"], asset["filename"])
        if "result" in result:
            # The video combine also returns its saved path (read by the continuity publisher).
            result["result"] = tuple(
                os.path.join(os.path.dirname(v), renamed[os.path.basename(v)])
                if isinstance(v, str) and os.path.basename(v) in renamed else v
                for v in result["result"]
            )
        return result
    return run


def prepare(data):
    if data.get("extra_data", {}).get("extra_pnginfo", {}).get("deas_output_naming"):
        for name in SAVERS:
            cls = nodes.NODE_CLASS_MAPPINGS.get(name)
            function = getattr(cls, "FUNCTION", None)
            save = getattr(cls, function, None) if isinstance(function, str) else None
            if callable(save) and not getattr(save, "_deas_naming", False):
                wrapped = wrap_save(save)
                wrapped._deas_naming = True
                setattr(cls, cls.FUNCTION, wrapped)
    return data


PromptServer.instance.add_on_prompt_handler(prepare)
NODE_CLASS_MAPPINGS = {}
