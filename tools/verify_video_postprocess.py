#!/usr/bin/env python3
"""Offline CPU proof that the video baseline (B7) is encoded before any post-processing node runs.

Real core executor + real DaSiWa_EnhancedVideoCombine from a node-pack checkout + the DEAS adapter;
synthetic tiny frames and audio, no models, no server, private scratch output folder."""
import argparse
import asyncio
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import types

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--comfyui-root', type=Path, required=True)
parser.add_argument('--node-pack', type=Path, required=True, help='ComfyUI-DaSiWa-Nodes checkout; imported without package startup.')
parser.add_argument('--scratch', type=Path, required=True)
args = parser.parse_args()
os.environ['PYTHONDONTWRITEBYTECODE'] = '1'
sys.dont_write_bytecode = True
sys.path.insert(0, str(args.comfyui_root.resolve()))
sys.argv = [sys.argv[0], '--cpu']
from comfy.cli_args import args as comfy_args
comfy_args.cpu = True
import folder_paths
import nodes
import execution
import comfy.model_management
import torch


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


adapter = load('deas_image_postprocess', Path(__file__).resolve().parents[1] / 'src-tauri/src/image_postprocess.py')
nodes.NODE_CLASS_MAPPINGS.update(adapter.NODE_CLASS_MAPPINGS)
package = types.ModuleType('_deas_pp1c_pack')
package.__path__ = [str(args.node_pack.resolve() / 'nodes')]
sys.modules[package.__name__] = package
# The saver module registers preview routes at import; give it a route table without starting a server.
import server
from aiohttp import web
server.PromptServer.instance = types.SimpleNamespace(routes=web.RouteTableDef(), send_sync=lambda *a, **k: None)
combine = load(package.__name__ + '.nodes_enhanced_video_combine', Path(package.__path__[0]) / 'nodes_enhanced_video_combine.py')
Combine = combine.DaSiWa_EnhancedVideoCombine
assert Combine.RETURN_TYPES == ('IMAGE', 'STRING'), Combine.RETURN_TYPES
seamless = load(package.__name__ + '.nodes_seamless_loop', Path(package.__path__[0]) / 'nodes_seamless_loop.py')
RealLoop = seamless.DaSiWa_SeamlessLoop
assert RealLoop.RETURN_TYPES == ('IMAGE', 'AUDIO'), 'node pack without the audio-aware Seamless Loop (needs 0.5.41+)'

calls = []
FRAMES, W, H, FPS, RATE = 9, 64, 48, 12.0, 16000


def originals():
    return list(Path(folder_paths.get_output_directory()).glob('proof_original*'))


class Loader:
    @classmethod
    def INPUT_TYPES(cls):
        return {'required': {'mode': ('STRING',)}}
    RETURN_TYPES = ('STRING',)
    FUNCTION = 'load'

    def load(self, mode):
        calls.append('loader')
        assert originals(), 'post root ran before the baseline video was encoded'
        if mode == 'loader-fail':
            raise RuntimeError('synthetic independent loader failure')
        return (mode,)


class Post:
    @classmethod
    def INPUT_TYPES(cls):
        return {'required': {'image': ('IMAGE',), 'loader': ('STRING',)}}
    RETURN_TYPES = ('IMAGE',)
    FUNCTION = 'run'

    def run(self, image, loader):
        calls.append('post')
        if loader == 'fail':
            raise RuntimeError('synthetic downstream failure')
        if loader == 'interrupt':
            raise comfy.model_management.InterruptProcessingException()
        return (1 - image,)


class Audio:
    @classmethod
    def INPUT_TYPES(cls):
        return {'required': {}}
    RETURN_TYPES = ('AUDIO',)
    FUNCTION = 'make'

    def make(self):
        t = torch.arange(int(RATE * FRAMES / FPS)) / RATE
        return ({'waveform': (0.2 * torch.sin(2 * torch.pi * 440 * t)).reshape(1, 1, -1), 'sample_rate': RATE},)


class Loop:
    """Stand-in for DaSiWa_SeamlessLoop: IMAGE+AUDIO in/out behind its own loader root."""
    @classmethod
    def INPUT_TYPES(cls):
        return {'required': {'image': ('IMAGE',), 'audio': ('AUDIO',), 'loader': ('STRING',)}}
    RETURN_TYPES = ('IMAGE', 'AUDIO')
    FUNCTION = 'run'

    def run(self, image, audio, loader):
        calls.append('loop')
        return (image, audio)


class Frames:
    """Closed footage (first == last): the real loop drops the duplicate end frame without loading an interpolator."""
    @classmethod
    def INPUT_TYPES(cls):
        return {'required': {}}
    RETURN_TYPES = ('IMAGE',)
    FUNCTION = 'make'

    def make(self):
        frames = torch.linspace(0, 1, FRAMES).reshape(-1, 1, 1, 1).expand(FRAMES, H, W, 3).clone()
        frames[-1] = frames[0]
        return (frames,)


class GuardedLoop(RealLoop):
    def execute(self, *a, **k):
        calls.append('loop')
        assert originals(), 'real loop ran before the baseline video was encoded'
        return super().execute(*a, **k)


class BrokenCombine(Combine):
    def combine(self, *a, **k):
        raise RuntimeError('synthetic baseline saver failure')


nodes.NODE_CLASS_MAPPINGS.update(DaSiWa_EnhancedVideoCombine=Combine, PP_Loader=Loader, PP_Post=Post, PP_Audio=Audio, PP_Loop=Loop, PP_BrokenCombine=BrokenCombine, PP_Frames=Frames, DaSiWa_SeamlessLoop=GuardedLoop)


class Server:
    client_id = None
    last_node_id = None

    def send_sync(self, *a):
        pass


def saver(class_type, images, prefix):
    return {'class_type': class_type, 'inputs': {
        'images': images, 'frame_rate': FPS, 'codec': 'H.264', 'container': 'MP4', 'bit_depth': '8-bit', 'quality': 20,
        'log_level': 'Standard', 'pingpong': False, 'save_metadata': True, 'filename_prefix': prefix, 'save_output': True,
        'pass_frames': False, 'crop_to_audio': False, 'audio_codec': 'Auto', 'audio_bitrate': '192k',
        'save_first_frame': False, 'save_last_frame': False, 'seed': 7, 'audio': ['A', 0]}}


# Same shape jobs_video.rs emits: B7 on the decode, post chain (with its own loader root) → PG(baseline=B7.filename, lazy image=chain) → 7.
# loop: seamless loop first (LP), its audio through the gate's lazy audio input (PG[1]) unless audio_gated is False.
def graph(mode, gated=True, baseline='DaSiWa_EnhancedVideoCombine', loop=False, audio_gated=True):
    g = {'E': {'class_type': 'EmptyImage', 'inputs': {'width': W, 'height': H, 'batch_size': FRAMES, 'color': 0}},
         'A': {'class_type': 'PP_Audio', 'inputs': {}}}
    previous = 'E'
    for n in range(8):
        g[f'D{n}'] = {'class_type': 'ImageInvert', 'inputs': {'image': [previous, 0]}}
        previous = f'D{n}'
    g['B7'] = saver(baseline, [previous, 0], 'proof_original')
    g['RL'] = {'class_type': 'PP_Loader', 'inputs': {'mode': mode}}
    g['R6'] = {'class_type': 'PP_Post', 'inputs': {'image': [previous, 0], 'loader': ['RL', 0]}}
    g['PG'] = {'class_type': 'DEAS_VideoPostprocessGate', 'inputs': {'baseline': ['B7', 1], 'image': ['R6', 0]}}
    g['7'] = saver('DaSiWa_EnhancedVideoCombine', ['PG' if gated else 'R6', 0], 'proof_processed')
    if loop:
        g['LP'] = {'class_type': 'PP_Loop', 'inputs': {'image': [previous, 0], 'audio': ['A', 0], 'loader': ['RL', 0]}}
        g['R6']['inputs']['image'] = ['LP', 0]
        if audio_gated:
            g['PG']['inputs']['audio'] = ['LP', 1]
        g['7']['inputs']['audio'] = ['PG', 1] if audio_gated else ['LP', 1]
    if not gated:
        del g['PG']
    return g


def probe(path):
    out = subprocess.run(['ffprobe', '-v', 'error', '-count_frames', '-show_entries',
                          'stream=codec_type,width,height,avg_frame_rate,nb_read_frames,sample_rate', '-of', 'json', str(path)],
                         check=True, capture_output=True, text=True).stdout
    streams = json.loads(out)['streams']
    video = next(s for s in streams if s['codec_type'] == 'video')
    audio = [s for s in streams if s['codec_type'] == 'audio']
    return (video['width'], video['height'], video['avg_frame_rate'], int(video['nb_read_frames']), len(audio))


args.scratch.mkdir(parents=True, exist_ok=True)
results = []
with tempfile.TemporaryDirectory(prefix='deas-pp1c-', dir=args.scratch) as directory:
    for setter in (folder_paths.set_output_directory, folder_paths.set_temp_directory, folder_paths.set_input_directory, folder_paths.set_user_directory):
        setter(directory)

    def clear():
        calls.clear()
        for p in Path(directory).iterdir():
            p.unlink()

    def run(g, expected, baseline=True, processed=False, real_loop=False):
        valid = asyncio.run(execution.validate_prompt('proof', g, None))
        assert valid[0], valid
        executor = execution.PromptExecutor(Server(), cache_type=execution.CacheType.CLASSIC, cache_args={'ram': 0, 'ram_inactive': 0, 'lru': 128})
        executor.execute(copy.deepcopy(g), 'proof', execute_outputs=['7', 'B7'])
        outputs = executor.history_result['outputs']
        assert executor.success is expected, executor.status_messages
        assert ('B7' in outputs) is baseline, outputs
        assert ('7' in outputs) is processed, outputs
        for node_id in outputs:
            for item in outputs[node_id].get('gifs', []):
                # Both products: same geometry, fps, frame count and a muxed audio stream.
                frames = FRAMES - 1 if node_id == '7' and real_loop else FRAMES
                assert probe(Path(directory) / item['subfolder'] / item['filename']) == (W, H, '12/1', frames, 1), (node_id, probe(Path(directory) / item['filename']))
        return executor

    clear()
    run(graph('loader-fail', gated=False), False, baseline=False)
    assert calls == ['loader'], calls
    results.append('sibling negative control: independent post root runs before the baseline exists')
    for mode in ['ok', 'loader-fail', 'fail', 'interrupt']:
        clear()
        executor = run(graph(mode), mode == 'ok', processed=mode == 'ok')
        assert calls == (['loader'] if mode == 'loader-fail' else ['loader', 'post']), calls
        assert len(originals()) == 1
        if mode == 'interrupt':
            assert any(event == 'execution_interrupted' for event, _ in executor.status_messages)
        results.append(f'{mode}: baseline encoded first and retained')
    clear()
    run(graph('loader-fail', loop=True, audio_gated=False), False, baseline=False)
    assert calls == ['loader'], calls
    results.append('loop negative control: saver audio straight from the loop pulls the post root before the baseline')
    for mode in ['ok', 'fail']:
        clear()
        run(graph(mode, loop=True), mode == 'ok', processed=mode == 'ok')
        assert calls == ['loader', 'loop', 'post'], calls
        assert len(originals()) == 1
        results.append(f'loop {mode}: looped audio passes the gate, baseline encoded first and retained')
    # Real DaSiWa_SeamlessLoop wired exactly as jobs_video.rs emits it (LP first, audio through PG[1]).
    for mode in ['Crossfade', 'Beat-aware']:
        clear()
        g = graph('ok', loop=True)
        g['F'] = {'class_type': 'PP_Frames', 'inputs': {}}
        g['B7']['inputs']['images'] = ['F', 0]
        g['LP'] = {'class_type': 'DaSiWa_SeamlessLoop', 'inputs': {
            'images': ['F', 0], 'audio': ['A', 0], 'frame_rate': FPS, 'audio_loop': mode, 'exact_endpoint': False,
            'interpolation_backend': 'RIFE / FILM', 'model_name': RealLoop.INPUT_TYPES()['optional']['model_name'][0][0]}}
        g['R6']['inputs']['image'] = ['LP', 0]
        executor = run(g, True, processed=True, real_loop=True)
        assert sorted(calls) == ['loader', 'loop', 'post'], calls  # LP and RL are independent roots behind the gate
        processed = next(Path(directory).glob('proof_processed*'))
        samples = subprocess.run(['ffprobe', '-v', 'error', '-select_streams', 'a:0', '-show_entries', 'stream=duration', '-of', 'csv=p=0', str(processed)],
                                 check=True, capture_output=True, text=True).stdout.strip()
        assert abs(float(samples) - (FRAMES - 1) / FPS) < 0.03, samples
        results.append(f'real loop {mode}: closed clip trimmed to {FRAMES - 1} frames, audio {samples}s through PG[1], baseline first')
    clear()
    run(graph('ok', baseline='PP_BrokenCombine'), False, baseline=False)
    assert not calls, calls
    results.append('baseline saver failure blocks every post root')
print(json.dumps({'python': sys.version.split()[0], 'torch': torch.__version__, 'device': str(comfy.model_management.get_torch_device()), 'passed': len(results), 'checks': results}, indent=2))
