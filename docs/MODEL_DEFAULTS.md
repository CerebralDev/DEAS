# Model Defaults Matrix — DEAS vs researched recommendations

Research date: 3 November 2026 (web research; source per row). Current column is the verbatim
`defaults` block of `src/families/builtin.json` (steps / CFG / sampler / scheduler / size).
Recommendation column prefers the official model card / official docs, then the official Comfy-Org
workflow template; community canon is listed only where no official source exists.
Verdicts drive the PLAN.md §4.5 checklist ("Verify every family default against real suggested
defaults"); nothing in Studio changes because of this file.

| Family (builtin.json id) | DEAS current | Researched recommendation | Verdict | Source |
|---|---|---|---|---|
| sd15 | 512², 20, CFG 7, dpmpp_2m, karras | Community canon 20 / 7 / dpmpp_2m / karras; ComfyUI default graph 20 / 8 / euler / normal | keep (community canon) | [Comfy-Org default graph](https://github.com/Comfy-Org/workflow_templates/blob/main/archived/default.json), [community sampler chart](https://www.reddit.com/r/StableDiffusion/comments/1fooj3x/choosing_a_sampler_and_scheduler_for_your_image/) |
| sdxl | 1024², 28, CFG 3.5, euler, normal | Template 25 / 8 / euler / normal; community 20–28 / 5–7.5 / dpmpp_2m / karras | user preference: euler + CFG 3.5 (Illustrious canon) | [sdxl_simple_example.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/sdxl_simple_example.json) |
| pony (SDXL sub-family) | sdxl values | Card: 25 steps, euler_a, quality tags (`score_9, score_8_up, source_anime`) in the positive prompt, CFG ~7 (community) | keep; prompt-style note | [Pony Diffusion V6 XL](https://huggingface.co/LyliaEngine/Pony_Diffusion_V6_XL) |
| illustrious (SDXL sub-family) | sdxl values (CFG 3.5 default by user preference) | Card: 20–28 steps, CFG 5–7.5, euler_a; negative example on card | adopt card negative; DEAS CFG 3.5 = community Illustrious practice | [Illustrious-XL v0](https://huggingface.co/OnomaAIResearch/Illustrious-xl-early-release-v0) |
| sd3 (SD3.5) | 1024², 28, CFG 4.5, dpmpp_2m, karras | Official template 20 / 4.01 / euler / sgm_uniform (SD3 shift 3.0); community canon 28 / 4.5 / dpmpp_2m / karras | keep (community canon); shift 3.0 already declared | [sd3.5_simple_example.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/sd3.5_simple_example.json), [ComfyUI_examples SD3](https://comfyanonymous.github.io/ComfyUI_examples/sd3/) |
| anima | 1024², 30, CFG 4, er_sde, simple | Official 30 / 4 / euler (template ships er_sde) / simple; negative `worst quality, low quality, score_1, score_2, score_3, blurry, jpeg artifacts, sepia`; turbo branch 8 / CFG 1 | keep; adopt recommended negative | [Comfy Anima docs](https://docs.comfy.org/tutorials/image/anima/anima), [image_anima_base_v1.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_anima_base_v1.json) |
| moss-image | 1024², 25, CFG 2.5, er_sde, simple | **Official card: 50+ steps, CFG 3.0 (3.0–6.0), euler + sgm_uniform, shift 5.0, SLG `uncond_skip_layers=[22]`, negative `nsfw, worst quality, bad anatomy, watermark, jpeg artifact`; 8-step DMD LoRA / 4-step rapid at CFG 1.** Comfy-Org template ships exactly the DEAS values (25 / 2.5 / er_sde / simple + SLG layer 22 scale 0.5) | **fix (card canonical, PLAN §4.5)** | [MOSS-Image USERGUIDE](https://github.com/OpenMOSS/MOSS-Image/blob/main/USERGUIDE.md), [comfyui/README](https://github.com/OpenMOSS/MOSS-Image/blob/main/comfyui/README.md) |
| ideogram4 | 1024², 20, CFG 4, euler, simple | Official: Default preset 20 steps (Quality 48 / Turbo 12), CFG 7 via DualModelGuider, Ideogram4Scheduler (mu 0.0 / std 1.75), dims multiples of 16 | **fix CFG 4→7**; scheduler is a dedicated node, not KSampler simple | [Comfy Ideogram docs](https://docs.comfy.org/tutorials/image/ideogram/ideogram-v4), [ComfyUI-Ideogram4 README](https://github.com/ideogram-oss/ComfyUI-Ideogram4/blob/main/README.md) |
| fibo2 | 1024², 20, CFG 4, euler, simple | Official dev 50 / CFG 5 / euler / simple; edit-turbo 4 / CFG 1 | **fix 20→50, 4→5 (dev)** | [image_fibo2_edit_dev_pe.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_fibo2_edit_dev_pe.json), [briaai/Fibo-2.0-dev](https://huggingface.co/briaai/Fibo-2.0-dev) |
| ernie | 1024², 20, CFG 4, euler, simple | Official 20 / 4 / euler / simple; turbo 8 / CFG 1 | keep | [image_ernie_image.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_ernie_image.json) |
| omnigen2 | 1024², 20, CFG 4, euler, simple | Official usage tips 50 steps, CFG 4.0, euler (dpmsolver++ for fewer steps), negative `blurry, low quality, text, watermark` | **fix 20→50**; adopt negative | [BAAI/OmniGen2](https://huggingface.co/BAAI/OmniGen2), [example_t2i.sh](https://github.com/VectorSpaceLab/OmniGen2/blob/main/example_t2i.sh) |
| boogu | 1024², 20, CFG 4, euler, simple | Official turbo: 4 steps, CFG 1, lcm, sgm_uniform; Base variant unverified | **fix to turbo defaults (4 / 1 / lcm / sgm_uniform)** | [Comfy Boogu docs](https://docs.comfy.org/tutorials/image/boogu/boogu-image-0.1), [turbo template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_boogu_image_0_1_turbo_t2i.json) |
| kandinsky5-image | 1024², 20, CFG 4, euler, simple | Official 50 / CFG 3.5 / euler / simple, shift 3.0 (ModelSamplingSD3) | **fix 20→50, 4→3.5; shift 3.0** | [image_kandinsky5_t2i.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_kandinsky5_t2i.json) |
| pixart | 1024², 20, CFG 4.5, dpmpp_2m, normal | Community 20–30 / 4.5–6 / dpmpp_2m / karras (no official sampling published) | keep (community) | [PixArt-Sigma card](https://huggingface.co/PixArt-alpha/PixArt-Sigma-XL-2-1024-MS) (resolution only) |
| hunyuan-dit | 1024², 30, CFG 5, dpmpp_2m, normal | Official app 100 / CFG 6 / ddpm; distilled 25; Comfy community 20–30 / 4.5–5.5 | keep (community; matches distilled band) | [Tencent/HunyuanDiT](https://github.com/Tencent/HunyuanDiT) |
| hidream | 1024², 50, CFG 5, euler, normal | Official Full 50 / 5 / **uni_pc** / **simple** / **shift 3.0**; Dev 28 / CFG 1 / shift 6; Fast 16 / CFG 1 | **fix sampler+scheduler+shift (Full)**; Dev/Fast need CFG 1 path | [Comfy HiDream docs](https://docs.comfy.org/tutorials/image/hidream/hidream-i1) |
| pixeldit | 1024², 20, CFG 4, euler, simple | Official 30 / CFG 4 / **er_sde** / simple, negative `low quality, worst quality, over-saturated, blurry, deformed, watermark` | **fix 20→30, euler→er_sde**; adopt negative | [image_pixeldit_t2i.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_pixeldit_t2i.json) |
| mage (Mage-Flow) | 1024², 30, CFG 5, euler, simple | Official 30 / 5 / euler / simple; turbo 4 / CFG 1 | keep (already the blueprint) | [image_mage_flow_t2i_int8.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_mage_flow_t2i_int8.json) |
| flux | 1024², 20, CFG 1, euler, simple | Official 20 / CFG 1 / euler / simple, ModelSamplingFlux shift 1.15 + FluxGuidance 3.5 (the "shift 3.0" lore is SD3-era) | keep; shift 1.15 | [flux_dev_full_text_to_image.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/flux_dev_full_text_to_image.json) |
| flux2 | 1024², 20, CFG 5, euler, simple | Official Klein base 20 / CFG 5 / euler + Flux2Scheduler (built-in shift, no ModelSampling node); distilled 4 / CFG 1; Dev 20 / CFG 1 + FluxGuidance 4 | keep | [image_flux2_klein_text_to_image.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_flux2_klein_text_to_image.json) |
| flux3 | 1024², 20, CFG 1, euler, simple | Official template: t2i 15 / CFG 1 / **er_sde** / simple; image-edit 50 / euler | **fix 20→15, euler→er_sde (t2i)** | [image_flux3_klein_t2i.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_flux3_klein_t2i.json) |
| chroma | 1024², 26, CFG 4, euler, simple | Official 26 / **CFG 3.5** / euler / **beta** / Flow-Shift 1.0 | **fix CFG 4→3.5, simple→beta, shift 1.0** | [image_chroma_text_to_image.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_chroma_text_to_image.json), [Chroma1-HD card](https://huggingface.co/lodestones/Chroma1-HD) |
| lumina2 (NetaYume) | 1024², 30, CFG 4, res_multistep, simple | Official 30 / 4 / res_multistep / simple, shift 4.0 (ModelSamplingAuraFlow) | keep; shift 4.0 | [image_netayume_lumina_t2i.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_netayume_lumina_t2i.json) |
| z-image | 1024², 8, CFG 1, res_multistep, simple | Official Turbo 8 / 1 / res_multistep / simple, shift 3.0 (base 25 / CFG 4) | keep; shift 3.0 | [image_z_image_turbo.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_z_image_turbo.json) |
| krea-2 | 1024², 8, CFG 1, euler, simple | Official turbo 8 / 1 / euler / simple; RAW 52 steps (CFG community 2–3, unverified) | keep (turbo default) | [Comfy Krea 2 docs](https://docs.comfy.org/tutorials/image/krea/krea-2), [image_krea2_turbo_t2i.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_krea2_turbo_t2i.json) |
| qwen-image-2.1 | 2048², 20, CFG 4, euler, simple | Official 25 steps, CFG 1 (raise only when a negative prompt is used), euler / simple, native 2048; Turbo 8 with fixed ManualSigmas | **fix CFG 4→1, steps 20→25** | [Qwen-Image 2.1 docs](https://docs.comfy.org/tutorials/image/qwen/qwen-image-2-1), [image_qwen_image_2_1_t2i.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_qwen_image_2_1_t2i.json) |
| qwen-image-2.1-turbo | 1024², 8, CFG 1, euler, simple | Official turbo 8 / CFG 1 (fixed schedule) | keep | [Qwen-Image-2.1-Turbo](https://huggingface.co/Qwen/Qwen-Image-2.1-Turbo) |
| minimax-h3 | 1024×768, 4, CFG 1, euler, simple | Official template base 30 steps (**8-step turbo LoRA** per template note; community lightning LoRAs run 4), **res_multistep** / simple, shift 1, CFG via BasicGuider (CFG-1 path with the LoRA) | **split: base (no LoRA) 30/res_multistep/CFG ~3; turbo variant keeps 4 (or 8 with the official 8-step LoRA), CFG 1.** DEAS's 4/CFG 1 is the LoRA value, correct only with the turbo/lightning LoRA loaded — as a bare default it under-denoises the base model | [image_minimax_h3_t2i.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_minimax_h3_t2i.json), [MiniMaxAI/MiniMax-H3](https://huggingface.co/MiniMaxAI/MiniMax-H3) |
| wan | 720×1280, 4, CFG 1, euler, linear_quadratic | Official 14B lightning 4 steps (2+2 stages) / CFG 1 / euler, ModelSamplingSD3 shift 5.0, scheduler simple; 5B 20 / CFG 5 / uni_pc / shift 8 | keep steps; **scheduler simple, shift 5.0 (14B)** | [video_wan2_2_14B_t2v.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/video_wan2_2_14B_t2v.json) |
| ltx-2 | 1280×704, 8, CFG 1, euler_cfg_pp, linear_quadratic | Not covered by official Comfy-Org templates in this research pass | unverified | — |
| sam3 | (no sampling defaults) | Segmentation model, no KSampler path | n/a | — |
| ace-step | (variant-specific music profiles) | Variant-specific; see ACE-Step table below. Turbo values are not valid generic defaults. | **applied per resolved variant; unresolved identities blocked** | [Comfy ACE-Step 1.0](https://docs.comfy.org/tutorials/audio/ace-step/ace-step-v1), [Comfy ACE-Step 1.5](https://docs.comfy.org/tutorials/audio/ace-step/ace-step-v1-5) |
| yue2 | 1024², 32, CFG 1, dpm_2, sgm_uniform | Official template identical (music model; the 32/1/dpm_2/sgm_uniform is its KSampler leg) | keep | [audio_yue2_text2music.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/audio_yue2_text2music.json) |

## ACE-Step: variant-specific reference settings

These are the shipped per-variant music starting settings; template-backed rows follow official Comfy-Org graphs. Regular Base/SFT uses a documented DEAS step choice within the manufacturer’s recommendation. Training variants cannot be established from shared tensor shapes alone. AIO/split is a loader form, not a different training variant; the XL DiT and code-LM size are independent.

| Variant | Steps / diffusion CFG | Sampler / scheduler | Sampling patch | Source |
| --- | --- | --- | --- | --- |
| Original 1.0 | 50 / 5 | euler / simple | ModelSamplingSD3, shift 5 | [original song template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/audio_ace_step_1_t2a_song.json) |
| 1.5 Base / SFT | 50 / 7 | euler / simple | ModelSamplingAuraFlow, shift 1 | [manufacturer inference parameters](https://github.com/ace-step/ACE-Step-1.5/blob/main/docs/en/INFERENCE.md#diffusion-parameters): CFG 7 / shift 1 for Base/SFT; DEAS chooses 50 within recommended 32–64 steps |
| 1.5 Turbo | 8 / 1 | euler / simple | ModelSamplingAuraFlow, shift 3 | [split Turbo template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/audio_ace_step_1_5_split.json) |
| 1.5 XL Base | 50 / 6 | euler / simple | ModelSamplingAuraFlow, shift 3 | [XL Base template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/audio_ace_step1_5_xl_base.json) |
| 1.5 XL SFT | 50 / 7 | euler / simple | ModelSamplingAuraFlow, shift 3 | [XL SFT template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/audio_ace_step1_5_xl_sft.json) |
| 1.5 XL Turbo | 8 / 1 | euler / simple | ModelSamplingAuraFlow, shift 3 | [XL Turbo template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/audio_ace_step1_5_xl_turbo.json) |

Regular Base/SFT follows the manufacturer’s explicit Base/SFT parameter guidance, while XL rows follow their exact Comfy templates. Additional shift/continuous distilled forms remain unresolved rather than guessed Turbo. Do not copy the XL Base CFG into regular Base or SFT. The 1.5 encoder's LM CFG (node default 2), temperature (0.85), top-p (0.9), top-k (0) and min-p (0) are separate from diffusion CFG; the installed core declares them in `TextEncodeAceStepAudio1.5`. Preserve per-model user overrides when revisiting a model.

## Fix list (feeds PLAN.md §4.5 checklist)

moss-image (card canonical, incl. SLG + negative), ideogram4 (CFG 7), fibo2 (50/5), omnigen2
(50), boogu (turbo 4/1/lcm/sgm_uniform), kandinsky5 (50/3.5 + shift 3), pixeldit (30/er_sde +
negative), hidream (uni_pc/simple/shift 3), flux3 (15/er_sde), chroma (3.5/beta/shift 1),
qwen-image-2.1 (CFG 1, 25 steps), minimax-h3 (30/res_multistep), wan (simple + shift 5),
ace-step music profiles are applied per resolved variant; remaining real-model acceptance is in PLAN F3. Recommended negatives to adopt: anima, omnigen2,
pixeldit, illustrious, moss.

## Caveats

- Comfy-Org templates and model cards conflict for MOSS (template 25/2.5/er_sde vs card
  50+/3/euler/sgm_uniform) and SD3.5 (template 20/4.01/euler/sgm_uniform vs community
  28/4.5/dpmpp_2m/karras). The matrix treats model cards as canonical and records the template
  value where DEAS already mirrors it.
- Flux "shift 3.0" is SD3-era lore; official Flux workflows use ModelSamplingFlux shift 1.15
  with FluxGuidance.
- Video/audio families beyond wan/ace-step/yue2 (ltx-2) and the H3 video paths were not audited.
- Pony/Illustrious are SDXL sub-families in DEAS, not separate default blocks.
