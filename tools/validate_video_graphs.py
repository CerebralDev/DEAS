"""Offline validation of the DEAS video graphs against the real ComfyUI checkout (no server, no GPU, custom nodes loaded).
Model/file names are swapped for ones the extra-model-paths config lists so combo checks pass; the point is node names, input names and link types."""
import argparse
import asyncio, json, sys, os
from pathlib import Path
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('graphs', type=Path)
parser.add_argument('--comfy-root', type=Path, required=True, help='Existing ComfyUI checkout')
parser.add_argument('--extra-model-paths-config', type=Path, required=True)
args = parser.parse_args()
# Resolve caller paths before ComfyUI changes the working directory/argv.
GRAPHS = args.graphs.resolve()
comfy_root = args.comfy_root.resolve()
model_config = args.extra_model_paths_config.resolve()
if not GRAPHS.is_file() or not (comfy_root / 'nodes.py').is_file() or not model_config.is_file():
    parser.error('graphs, ComfyUI nodes.py and model paths config must exist')
sys.argv = ['probe', '--cpu', '--extra-model-paths-config', str(model_config)]
os.chdir(comfy_root); sys.path.insert(0, str(comfy_root))
import comfy.options; comfy.options.enable_args_parsing()
import folder_paths, utils.extra_config
utils.extra_config.load_extra_path_config(str(model_config))
import server
loop = asyncio.new_event_loop(); asyncio.set_event_loop(loop)
from app.assets.manager import default_asset_manager
server.PromptServer(loop, default_asset_manager())  # custom nodes register routes on PromptServer.instance at import time
import nodes, execution
loop.run_until_complete(nodes.init_extra_nodes(init_custom_nodes=True))
graphs = json.loads(GRAPHS.read_text(encoding='utf-8'))
first = lambda kind: (folder_paths.get_filename_list(kind) or ['MISSING'])[0]
pick = {'unet': [n for n in folder_paths.get_filename_list('diffusion_models')], 'te': folder_paths.get_filename_list('text_encoders'), 'vae': folder_paths.get_filename_list('vae'), 'img': folder_paths.get_filename_list('input') if 'input' in folder_paths.folder_names_and_paths else [], 'lup': folder_paths.get_filename_list('latent_upscale_models')}
def sub(name, wants):
    for cand in pick[wants]:
        if all(k in cand.lower() for k in name):
            return cand
    return pick[wants][0] if pick[wants] else 'MISSING'
swap = {'LTX2/ltx.safetensors': sub(['ltx'], 'unet'), 'WAN22/high.safetensors': sub(['high'], 'unet'), 'WAN22/low.safetensors': sub(['low'], 'unet'), 'MiniMaxH3/fl2va.safetensors': sub(['h3'], 'unet'), 'MiniMaxH3/hyperflow_flash.gguf': 'deas-probe-hyperflow.gguf',
        'gemma_3_12B_it_fp8.safetensors': sub(['gemma'], 'te'), 'ltx-2.3_text_projection_bf16.safetensors': sub(['projection'], 'te'), 'umt5_xxl_fp8.safetensors': sub(['umt5'], 'te'), 'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors': sub(['minimax'], 'te'),
        'LTX2/video_vae.safetensors': sub(['ltx', 'video'], 'vae'), 'LTX2/audio_vae.safetensors': sub(['ltx', 'audio'], 'vae'), 'Wan/wan_2.1_vae.safetensors': sub(['wan'], 'vae'), 'MiniMaxH3/video_vae.safetensors': sub(['h3', 'video'], 'vae'), 'MiniMaxH3/audio_vae.safetensors': sub(['h3', 'audio'], 'vae'),
        'ltx-2.3-spatial-upscaler-x2-1.1.safetensors': sub(['spatial'], 'lup')}
images = sorted(os.listdir(folder_paths.get_input_directory()))
image = next((f for f in images if f.lower().endswith(('.png', '.jpg', '.webp'))), 'MISSING')
# Video-to-video needs a top-level input video (core LoadVideo lists only those): a scratch input folder with a
# 1 s ffmpeg test clip, so the real input folder stays untouched.
scratch_directory = tempfile.TemporaryDirectory(prefix='deas-validate-input-')
scratch = scratch_directory.name
import shutil, subprocess
shutil.copy(os.path.join(folder_paths.get_input_directory(), image), os.path.join(scratch, image))
if not os.path.exists(os.path.join(scratch, 'source.mp4')):
    subprocess.run(['ffmpeg', '-loglevel', 'error', '-f', 'lavfi', '-i', 'testsrc=size=320x240:rate=24:duration=1', '-pix_fmt', 'yuv420p', os.path.join(scratch, 'source.mp4')], check=True)
folder_paths.set_input_directory(scratch)
# The HyperFlow GGUF loader combo lists only .gguf files; nobody's disk is guaranteed to have
# one, so drop a probe file into the first writable diffusion_models folder (removed below).
probe_gguf = None
for d in folder_paths.get_folder_paths('diffusion_models'):
    candidate = os.path.join(d, 'deas-probe-hyperflow.gguf')
    try:
        open(candidate, 'wb').close(); probe_gguf = candidate; break
    except OSError:
        continue
ok = True
for family, g in graphs.items():
    text = json.dumps(g)
    for a, b in swap.items():
        text = text.replace(json.dumps(a)[1:-1], b.replace('\\', '\\\\'))
    text = text.replace('"ref.png"', json.dumps(image)).replace('"first.png"', json.dumps(image)).replace('"deas-inputs/a.png"', json.dumps(image))
    g = json.loads(text)
    result = loop.run_until_complete(execution.validate_prompt('deas-probe', g, None))
    valid, error, outputs, node_errors = result[0], result[1], result[2], result[3]
    print(f"{family}: valid={valid} outputs={outputs}")
    if not valid:
        ok = False
        print('ERR', error.get('message') if isinstance(error, dict) else error, (error.get('details') if isinstance(error, dict) else '')[:300])
        for nid, e in node_errors.items():
            print('NODE', nid, g[nid]['class_type'], json.dumps(e.get('errors'), ensure_ascii=False)[:600])
print('ALL VALID' if ok else 'INVALID')
if probe_gguf:
    os.remove(probe_gguf)
scratch_directory.cleanup()
sys.exit(0 if ok else 1)
