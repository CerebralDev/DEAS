"""Defer the complete optional image post-processing closure until saving succeeds."""


class ImagePostprocessGate:
    @classmethod
    def INPUT_TYPES(cls):
        return {"required": {
            "baseline": ("IMAGE,STRING",),
            "image": ("IMAGE", {"lazy": True}),
        }}

    RETURN_TYPES = ("IMAGE",)
    FUNCTION = "forward"
    CATEGORY = "DEAS/internal"

    def check_lazy_status(self, baseline, image=None):
        return ["image"] if image is None else []

    def forward(self, baseline, image):
        return (image,)


NODE_CLASS_MAPPINGS = {"DEAS_ImagePostprocessGate": ImagePostprocessGate}
