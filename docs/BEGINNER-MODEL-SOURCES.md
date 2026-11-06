# Beginner model-guide sources

Checked 2026-11-05. These editorial summaries describe model families and prompting approaches; they are not measured rankings or guarantees. Tag sets are local discovery metadata based on the published capabilities/examples. In particular, `anime` does not imply that the same prompt works equally well on all four models. No model weights are bundled.

Descriptions and prompting guidance were paraphrased from these primary sources:

- [Anima model card](https://huggingface.co/circlestone-labs/Anima), revision `f973fc41ec7545364ac9776c2440285f43ff2a30`: Base versions, non-photorealistic focus, tag/natural-language guidance.
- [MOSS SFT model card](https://huggingface.co/OpenMOSS-Team/MOSS-Image-SFT), revision `f972bd4b4b9e3852133cc5da954d87bc32cd8fb9`; [publisher user guide](https://github.com/OpenMOSS/MOSS-Image/blob/main/USERGUIDE.md): natural descriptions, tag sequences, text line breaks and optional layouts.
- [Krea 2 Raw model card](https://huggingface.co/krea/Krea-2-Raw), revision `6b0ece7fffb640c5e3bcbe0a7f10f66b8e60a603`; [official code](https://github.com/krea-ai/krea-2); [official demo prompting tips](https://huggingface.co/spaces/krea/Krea-2/blob/main/app.py): Raw/base distinction and descriptive language. The Raw weights/card raw endpoint is gated; the public card and official public code/demo were read without bypassing access controls.
- [Illustrious original base model card](https://huggingface.co/OnomaAIResearch/Illustrious-xl-early-release-v0), revision `dca0dac303e6dc4b0c31d8001bc685b89b5d0204`: original v0.1, Danbooru training and illustration focus. The concise tag-based starter advice is editorial guidance derived from that training description, rather than a verbatim publisher recipe.

Publisher illustrations are linked only for provenance and credit. They are not bundled, fetched by DEAS, or rendered in the guide cards. Cards show a local placeholder. Refer to the linked publisher pages for model/material terms (Anima's CircleStone license, MOSS Apache-2.0, Krea's model/material license and Illustrious's stated Fair AI Public License). Model licenses and generated-image usage are distinct. This package includes no weights and does not accept gated-model terms on the user's behalf.

## Publisher illustration links

- Anima: CircleStone Labs / Comfy Org — [preview illustration](https://huggingface.co/circlestone-labs/Anima/resolve/f973fc41ec7545364ac9776c2440285f43ff2a30/example.png). It comes from the preview checkpoint and should be understood as a family illustration, not a verified Base v1.0 output.
- MOSS SFT: OpenMOSS — [SFT comparison illustration](https://huggingface.co/OpenMOSS-Team/MOSS-Image-SFT/resolve/f972bd4b4b9e3852133cc5da954d87bc32cd8fb9/assets/comparison/sft.webp).
- Krea 2 Raw: Krea — [official Raw example 00](https://raw.githubusercontent.com/krea-ai/krea-2/main/assets/hf_samples/raw/00.png).
- Illustrious XL v0.1: OnomaAI Research — [official model-card sample s00](https://cdn-uploads.huggingface.co/production/uploads/65eea2d62cc24ebc6dbe16c0/dXvGxUKjcsqzt_gDWc9FU.png).

## Download file-list sources

Featured file allowlists were checked against the public repository API listings, with no weight downloads:

- Anima: `circlestone-labs/Anima`, revision above, `split_files/diffusion_models/anima-base-v1.0.safetensors`.
- MOSS SFT: [Comfy-Org/MOSS-Image](https://huggingface.co/Comfy-Org/MOSS-Image), revision `2929efe3cdd506c0a430bcfca70a9a8d805cc095`, `moss_image_sft_bf16.safetensors` and `moss_image_sft_int8_convrot.safetensors` under diffusion_models.
- Krea Raw: [Comfy-Org/Krea-2](https://huggingface.co/Comfy-Org/Krea-2), revision `588feaf955b08b9ac4723eeb751752ab045dba71`, the Raw bf16, fp8_scaled and int8_convrot files under diffusion_models.
- Illustrious: original publisher repo above, `Illustrious-XL-v0.1.safetensors`.

Native browse resolves the current upstream listing on an explicit card action. File availability, authentication and provider terms remain upstream-controlled. The allowlist is a version-selection filter; native validated source/resource identities and filesystem checks still govern any transfer.
