"""Defer the complete optional image/video post-processing closure until the baseline saver succeeds."""

_UNCONNECTED = object()


class ImagePostprocessGate:
    @classmethod
    def INPUT_TYPES(cls):
        return {"required": {
            "baseline": ("IMAGE,STRING",),
            "image": ("IMAGE", {"lazy": True}),
        }, "optional": {
            "audio": ("AUDIO", {"lazy": True}),
        }}

    RETURN_TYPES = ("IMAGE", "AUDIO")
    FUNCTION = "forward"
    CATEGORY = "DEAS/internal"

    # ComfyUI passes a connected, not yet evaluated lazy input as None and omits an unconnected
    # optional one; requesting an unconnected input is an execution error.
    def check_lazy_status(self, baseline, image=None, audio=_UNCONNECTED):
        return [name for name, value in (("image", image), ("audio", audio)) if value is None]

    def forward(self, baseline, image, audio=None):
        return (image, audio)


# Same ABI; the video name is advertised only when the video saver has its own post-save socket.
NODE_CLASS_MAPPINGS = {"DEAS_ImagePostprocessGate": ImagePostprocessGate, "DEAS_VideoPostprocessGate": ImagePostprocessGate}
