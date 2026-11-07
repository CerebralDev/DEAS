#!/usr/bin/env python3
"""Offline CPU scheduling proof against an installed ComfyUI (no services/models)."""
import argparse
import asyncio
import copy
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import types
from PIL import Image

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--comfyui-root', type=Path, required=True)
parser.add_argument('--scratch', type=Path, required=True)
parser.add_argument('--metadata-root', type=Path, help='Optional actual DaSiWa node-pack checkout; imported without package startup.')
parser.add_argument('--cache-type', choices=['CLASSIC', 'LRU', 'NONE', 'RAM_PRESSURE'], default='CLASSIC')
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

spec = importlib.util.spec_from_file_location('deas_image_postprocess', Path(__file__).resolve().parents[1] / 'src-tauri/src/image_postprocess.py')
assert spec is not None and spec.loader is not None
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)
nodes.NODE_CLASS_MAPPINGS.update(adapter.NODE_CLASS_MAPPINGS)
if args.metadata_root:
    package = types.ModuleType('_deas_pp1_metadata')
    package.__path__ = [str(args.metadata_root.resolve() / 'nodes')]
    sys.modules[package.__name__] = package
    metadata_spec = importlib.util.spec_from_file_location(package.__name__ + '.nodes_metadata', Path(package.__path__[0]) / 'nodes_metadata.py')
    assert metadata_spec is not None and metadata_spec.loader is not None
    metadata = importlib.util.module_from_spec(metadata_spec)
    metadata_spec.loader.exec_module(metadata)
    nodes.NODE_CLASS_MAPPINGS['DaSiWa_MetadataImageSaver'] = metadata.DaSiWa_MetadataImageSaver
    nodes.NODE_CLASS_MAPPINGS['DaSiWa_MetadataConfig'] = metadata.DaSiWa_MetadataConfig

# Concrete metadata-schema stub: delegates real disk encoding to core, then returns STRING.
class MetadataSaver(nodes.SaveImage):
    RETURN_TYPES = ('STRING',)
    def save_images(self, images, filename_prefix='ComfyUI', **kwargs):
        result = super().save_images(images, filename_prefix)
        return {**result, 'result': (result['ui']['images'][0]['filename'],)}

calls = []
class Loader:
    @classmethod
    def INPUT_TYPES(cls):
        return {'required': {'mode': ('STRING',)}}
    RETURN_TYPES = ('STRING',)
    FUNCTION = 'load'
    def load(self, mode):
        calls.append('loader')
        assert list(Path(folder_paths.get_output_directory()).glob('*original*.png')), 'post root ran before baseline publication'
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

class BrokenSaver(nodes.SaveImage):
    def save_images(self, images, filename_prefix='ComfyUI', **kwargs):
        raise RuntimeError('synthetic baseline saver failure')

class BrokenMetadataSaver(BrokenSaver):
    RETURN_TYPES = ('STRING',)

nodes.NODE_CLASS_MAPPINGS.update(PP_MetadataStub=MetadataSaver, PP_Loader=Loader, PP_Post=Post, PP_BrokenSaver=BrokenSaver, PP_BrokenMetadataSaver=BrokenMetadataSaver)
class Server:
    client_id = None
    last_node_id = None
    def send_sync(self, *args):
        pass

def graph(saver, mode, gated=True):
    g = {'E': {'class_type': 'EmptyImage', 'inputs': {'width': 16, 'height': 16, 'batch_size': 1, 'color': 0}}}
    previous = 'E'
    for n in range(12):
        name = f'D{n}'
        g[name] = {'class_type': 'ImageInvert', 'inputs': {'image': [previous, 0]}}
        previous = name
    g.update({
        'B7': {'class_type': saver, 'inputs': {'images': [previous, 0], 'filename_prefix': 'proof_original'}},
        'RL': {'class_type': 'PP_Loader', 'inputs': {'mode': mode}},
        'R6': {'class_type': 'PP_Post', 'inputs': {'image': [previous, 0], 'loader': ['RL', 0]}},
        'PG': {'class_type': 'DEAS_ImagePostprocessGate', 'inputs': {'baseline': ['B7', 0], 'image': ['R6', 0]}},
        '7': {'class_type': 'SaveImage', 'inputs': {'images': ['PG' if gated else 'R6', 0], 'filename_prefix': 'proof_processed'}},
    })
    if saver == 'DaSiWa_MetadataImageSaver':
        g['M'] = {'class_type': 'DaSiWa_MetadataConfig', 'inputs': {'save_workflow': True, 'model_hash': '', 'text_positive': 'PP1 metadata proof'}}
        for key in ['B7', '7']:
            g[key]['class_type'] = saver
            g[key]['inputs'].update(file_format='png', compression=44, save_output=True, metadata_config=['M', 0])
    if not gated:
        del g['PG']
    return g

args.scratch.mkdir(parents=True, exist_ok=True)
results = []
with tempfile.TemporaryDirectory(prefix='deas-pp1-', dir=args.scratch) as directory:
    folder_paths.set_output_directory(directory)
    folder_paths.set_temp_directory(directory)
    folder_paths.set_input_directory(directory)
    folder_paths.set_user_directory(directory)
    def clear():
        calls.clear()
        for p in Path(directory).glob('*.png'):
            p.unlink()
    def run(executor, g, expected, baseline=True, processed=False):
        valid = asyncio.run(execution.validate_prompt('proof', g, None))
        assert valid[0], valid
        executor.execute(copy.deepcopy(g), 'proof', execute_outputs=['7', 'B7'])
        outputs = executor.history_result['outputs']
        assert executor.success is expected, executor.status_messages
        assert ('B7' in outputs) is baseline, outputs
        assert ('7' in outputs) is processed, outputs
        for node_id, output in outputs.items():
            for image in output.get('images', []):
                with Image.open(Path(directory) / image['filename']) as encoded:
                    encoded.load()
                    assert encoded.size == (16, 16)
                    if g[node_id]['class_type'] == 'DaSiWa_MetadataImageSaver':
                        assert 'PP1 metadata proof' in encoded.info.get('parameters', ''), encoded.info
                    assert encoded.getpixel((0, 0)) == ((0, 0, 0) if node_id == 'B7' else (255, 255, 255))
        return outputs
    def fresh():
        return execution.PromptExecutor(Server(), cache_type=execution.CacheType[args.cache_type], cache_args={'ram': 0, 'ram_inactive': 0, 'lru': 128})
    for saver in ['SaveImage', 'PP_MetadataStub'] + (['DaSiWa_MetadataImageSaver'] if args.metadata_root else []):
        clear()
        # Negative control: the old sibling topology exposes the independent loader early.
        run(fresh(), graph(saver, 'loader-fail', False), False, baseline=False)
        assert calls == ['loader']
        results.append(f'{saver}: sibling negative control')
        for mode in ['ok', 'loader-fail', 'fail', 'interrupt']:
            clear()
            executor = fresh()
            g = graph(saver, mode)
            run(executor, g, mode == 'ok', processed=mode == 'ok')
            assert calls == (['loader'] if mode == 'loader-fail' else ['loader', 'post']), calls
            before = len(list(Path(directory).glob('*.png')))
            if mode == 'interrupt':
                assert any(event == 'execution_interrupted' for event, _ in executor.status_messages)
            calls.clear()
            run(executor, g, mode == 'ok', processed=mode == 'ok')
            expected_new = (2 if mode == 'ok' else 1) if args.cache_type == 'NONE' else 0
            assert len(list(Path(directory).glob('*.png'))) == before + expected_new
            if mode == 'ok':
                if args.cache_type != 'NONE':
                    assert not calls
                g['RL']['inputs']['mode'] = 'fail'
                run(executor, g, False)
                cached = next(data['nodes'] for event, data in executor.status_messages if event == 'execution_cached')
                assert ('B7' in cached) is (args.cache_type != 'NONE'), cached
            results.append(f'{saver}: {mode}, retained history and cache')
        clear()
        run(fresh(), graph('PP_BrokenSaver' if saver == 'SaveImage' else 'PP_BrokenMetadataSaver', 'ok'), False, baseline=False)
        assert not calls, calls
        results.append(f'{saver}: baseline failure blocks all post roots')
print(json.dumps({'python': sys.version.split()[0], 'torch': execution.torch.__version__, 'device': str(comfy.model_management.get_torch_device()), 'cache': args.cache_type, 'passed': len(results), 'checks': results}, indent=2))
