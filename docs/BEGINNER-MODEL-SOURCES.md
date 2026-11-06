# Beginner model-guide sources and illustration provenance

Checked 2026-11-05. These editorial summaries describe model families and prompting approaches; they are not measured rankings or guarantees. Tag sets are local discovery metadata based on the published capabilities/examples. In particular, `anime` does not imply that the same prompt works equally well on all four models. No model weights were downloaded or bundled.

Descriptions and prompting guidance were paraphrased from these primary sources:

- [Anima model card](https://huggingface.co/circlestone-labs/Anima), revision `f973fc41ec7545364ac9776c2440285f43ff2a30`: Base versions, non-photorealistic focus, tag/natural-language guidance.
- [MOSS SFT model card](https://huggingface.co/OpenMOSS-Team/MOSS-Image-SFT), revision `f972bd4b4b9e3852133cc5da954d87bc32cd8fb9`; [publisher user guide](https://github.com/OpenMOSS/MOSS-Image/blob/main/USERGUIDE.md): natural descriptions, tag sequences, text line breaks and optional layouts.
- [Krea 2 Raw model card](https://huggingface.co/krea/Krea-2-Raw), revision `6b0ece7fffb640c5e3bcbe0a7f10f66b8e60a603`; [official code](https://github.com/krea-ai/krea-2); [official demo prompting tips](https://huggingface.co/spaces/krea/Krea-2/blob/main/app.py): Raw/base distinction and descriptive language. The Raw weights/card raw endpoint is gated; the public card and official public code/demo were read without bypassing access controls.
- [Illustrious original base model card](https://huggingface.co/OnomaAIResearch/Illustrious-xl-early-release-v0), revision `dca0dac303e6dc4b0c31d8001bc685b89b5d0204`: original v0.1, Danbooru training and illustration focus. The concise tag-based starter advice is editorial guidance derived from that training description, rather than a verbatim publisher recipe.

The files below are publisher-distributed illustrations copied without visual alterations. Rendering uses CSS containment to preserve each full image. They retain their upstream provenance; this local DEAS patch does not relicense third-party material under GPL. Refer to the linked publisher pages for model/material terms (Anima's CircleStone license, MOSS Apache-2.0, Krea's model/material license and Illustrious's stated Fair AI Public License). Model licenses and generated-image usage are distinct. This package includes no weights and does not accept gated-model terms on the user's behalf.

## Anima

- Credit: CircleStone Labs / Comfy Org.
- [Original illustration](https://huggingface.co/circlestone-labs/Anima/resolve/f973fc41ec7545364ac9776c2440285f43ff2a30/example.png)
- Bundled file: `src/features/beginner/assets/anima.png`
- SHA-256: `eb08bb3259b6f4307ba1a89687e5b65fb2ca3c2ee2fef688a02bce194e40d977`
- The embedded PNG workflow uses anima-preview.safetensors. Displayed as a family illustration, not a verified Base v1.0 output.

## MOSS SFT

- Credit: OpenMOSS.
- [Original illustration](https://huggingface.co/OpenMOSS-Team/MOSS-Image-SFT/resolve/f972bd4b4b9e3852133cc5da954d87bc32cd8fb9/assets/comparison/sft.webp)
- Bundled file: `src/features/beginner/assets/moss.webp`
- SHA-256: `de1ba44f48d49e792b16aac26f2d260beefbcb9fbfa72f3ace363ff8a9917f71`
- Official SFT side of the Base/SFT comparison; the Base image is not included.

## Krea 2 Raw

- Credit: Krea.
- [Original illustration](https://raw.githubusercontent.com/krea-ai/krea-2/main/assets/hf_samples/raw/00.png)
- Bundled file: `src/features/beginner/assets/krea2.png`
- SHA-256: `2012df3238ac5d778084dc2118194cd8126802fda60ed5618818618b92dee2b9`
- Official Raw example 00, linked by the Raw model card. Not the Turbo gallery.

## Illustrious XL v0.1

- Credit: OnomaAI Research.
- [Original illustration](https://cdn-uploads.huggingface.co/production/uploads/65eea2d62cc24ebc6dbe16c0/dXvGxUKjcsqzt_gDWc9FU.png)
- Bundled file: `src/features/beginner/assets/illustrious.png`
- SHA-256: `0dec811cd284bea4c16017cdc167ae813f1336d58d429a8068581541258a211c`
- Sample s00 linked in the official original v0.1 model card, revision dca0dac303e6dc4b0c31d8001bc685b89b5d0204.

## Download file-list sources

Featured file allowlists were checked against the public repository API listings, with no weight downloads:

- Anima: `circlestone-labs/Anima`, revision above, `split_files/diffusion_models/anima-base-v1.0.safetensors`.
- MOSS SFT: [Comfy-Org/MOSS-Image](https://huggingface.co/Comfy-Org/MOSS-Image), revision `2929efe3cdd506c0a430bcfca70a9a8d805cc095`, `moss_image_sft_bf16.safetensors` and `moss_image_sft_int8_convrot.safetensors` under diffusion_models.
- Krea Raw: [Comfy-Org/Krea-2](https://huggingface.co/Comfy-Org/Krea-2), revision `588feaf955b08b9ac4723eeb751752ab045dba71`, the Raw bf16, fp8_scaled and int8_convrot files under diffusion_models.
- Illustrious: original publisher repo above, `Illustrious-XL-v0.1.safetensors`.

Native browse resolves the current upstream listing on an explicit card action. File availability, authentication and provider terms remain upstream-controlled. The allowlist is a version-selection filter; native validated source/resource identities and filesystem checks still govern any transfer.
