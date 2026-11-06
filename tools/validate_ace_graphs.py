"""Validate production-compiled ACE graphs against a real ComfyUI CPU node registry.

No server, weights, model execution, native services or real model folders are used.
Empty scratch files provide explicit filename combo fixtures: this proves graph schemas,
not model loading/inference. Run with an existing ComfyUI Python environment.
"""
import argparse
import asyncio
import json
import os
from pathlib import Path
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('graphs', type=Path)
    parser.add_argument('--comfy-root', required=True, type=Path)
    args = parser.parse_args()
    graphs_path = args.graphs.resolve()
    comfy_root = args.comfy_root.resolve()
    graphs = json.loads(graphs_path.read_text(encoding='utf-8'))
    if not graphs or not isinstance(graphs, dict) or not (comfy_root / 'nodes.py').is_file():
        parser.error('expected a nonempty named graph map and an existing ComfyUI checkout')
    scratch = Path(os.environ.get('TMPDIR', Path.home()/'.hermes/cache/scratch'))
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='deas-ace-schema-', dir=scratch) as temporary:
        root = Path(temporary)
        for directory in ('input', 'output', 'user', 'temp'):
            (root/directory).mkdir()
        sys.argv = ['deas-ace-schema', '--cpu', '--disable-all-custom-nodes', '--output-directory', str(root/'output'), '--input-directory', str(root/'input'), '--user-directory', str(root/'user'), '--temp-directory', str(root/'temp'), '--database-url', 'sqlite:///:memory:']
        os.chdir(comfy_root)
        sys.path.insert(0, str(comfy_root))
        import comfy.options
        comfy.options.enable_args_parsing()
        import folder_paths
        choices = {'CheckpointLoaderSimple': [('ckpt_name', 'checkpoints')],
                   'UNETLoader': [('unet_name', 'diffusion_models')],
                   'DualCLIPLoader': [('clip_name1', 'text_encoders'), ('clip_name2', 'text_encoders')],
                   'VAELoader': [('vae_name', 'vae')]}
        for category in ('checkpoints', 'diffusion_models', 'text_encoders', 'vae'):
            target = root/category
            target.mkdir()
            folder_paths.folder_names_and_paths[category] = ([str(target)], {'.safetensors'})
        for graph in graphs.values():
            for node in graph.values():
                for field, category in choices.get(node['class_type'], []):
                    name = node['inputs'][field].replace('\\', '/')
                    if name.startswith('/') or '..' in name.split('/'):
                        raise ValueError('unsafe filename in graph fixture')
                    target = root/category/name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.touch()
        import server
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        from app.assets.manager import default_asset_manager
        server.PromptServer(loop, default_asset_manager())
        import nodes
        import execution
        loop.run_until_complete(nodes.init_extra_nodes(init_custom_nodes=False))
        results = []
        for name, graph in graphs.items():
            result = loop.run_until_complete(execution.validate_prompt('deas-ace-schema', graph, None))
            valid, error, outputs, node_errors = result[:4]
            results.append({'name': name, 'valid': valid, 'error': error, 'outputs': outputs, 'nodeErrors': node_errors})
        loop.close()
        print(json.dumps({'kind': 'real ComfyUI CPU schema validation; scratch filename fixtures; no inference', 'results': results}, indent=2))
        return 0 if all(item['valid'] for item in results) else 1


if __name__ == '__main__':
    raise SystemExit(main())
