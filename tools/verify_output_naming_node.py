"""Save real CPU test pixels via core and DaSiWa savers using a DEAS-resolved prefix; no model/service."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

root=Path(__file__).resolve().parents[1]
comfy=Path(sys.argv[1]).resolve()
out=Path(sys.argv[2]).resolve()
out.mkdir(parents=True,exist_ok=True)
js="""import {outputPrefix,defaultOutputNaming} from './src/lib/outputNaming.ts';
console.log(outputPrefix(defaultOutputNaming,{type:'image',seed:42,model:'folder/My Model.safetensors'},new Date(2026,0,2,3,4,5)));"""
prefix=subprocess.check_output(['node','--experimental-strip-types','--input-type=module','-e',js],cwd=root,text=True,encoding='utf-8').strip()
assert prefix=='raw/image/2026-01-02/03-04-05-42-My-Model',prefix
sys.argv=['output-naming-probe','--cpu']
sys.path.insert(0,str(comfy))
import folder_paths
import nodes
import torch
from PIL import Image
folder_paths.set_output_directory(str(out))
pack=comfy/'custom_nodes/ComfyUI-DaSiWa-Nodes'
sys.path.insert(0,str(pack/'nodes'))
spec=importlib.util.spec_from_file_location('naming_metadata_probe',pack/'nodes/nodes_metadata.py')
assert spec and spec.loader
metadata=importlib.util.module_from_spec(spec)
spec.loader.exec_module(metadata)
adapter_spec=importlib.util.spec_from_file_location('deas_output_naming',root/'src-tauri/src/output_naming.py')
adapter=importlib.util.module_from_spec(adapter_spec)
# The probe has no server instance; only its registration point is stubbed.
import server
from types import SimpleNamespace
server.PromptServer.instance=SimpleNamespace(add_on_prompt_handler=lambda handler:None,routes=SimpleNamespace(get=lambda path:lambda f:f))
adapter_spec.loader.exec_module(adapter)
nodes.NODE_CLASS_MAPPINGS['DaSiWa_MetadataImageSaver']=metadata.DaSiWa_MetadataImageSaver
video_spec=importlib.util.spec_from_file_location('naming_video_probe',pack/'nodes/nodes_enhanced_video_combine.py')
video=importlib.util.module_from_spec(video_spec)
video_spec.loader.exec_module(video)
nodes.NODE_CLASS_MAPPINGS['DaSiWa_EnhancedVideoCombine']=video.DaSiWa_EnhancedVideoCombine
request={'extra_data':{'extra_pnginfo':{'deas_output_naming':True}}}
assert adapter.prepare(request) is request
wrapped=nodes.SaveImage.save_images
adapter.prepare(request)
assert nodes.SaveImage.save_images is wrapped
paths=[]
for saver in (nodes.SaveImage(),metadata.DaSiWa_MetadataImageSaver()):
 for _ in range(2):
  kwargs={} if isinstance(saver,nodes.SaveImage) else {'file_format':'png','compression':44,'save_output':True,'prompt':{'7':{'class_type':'DaSiWa_MetadataImageSaver','inputs':{'filename_prefix':prefix}}}}
  saved=saver.save_images(extra_pnginfo={'deas_output_naming':True},images=torch.zeros((1,8,8,3)),filename_prefix=prefix,**kwargs)
  entry=saved['ui']['images'][0]
  path=out/entry['subfolder']/entry['filename']
  assert entry['subfolder']=='raw/image/2026-01-02',entry
  assert entry['filename'].startswith('03-04-05-42-My-Model-') and entry['filename'].endswith('.png') and '_' not in entry['filename'],entry
  assert path not in paths,'counter reused a filename'
  with Image.open(path) as image:assert image.size==(8,8)
  paths.append(path)
# Video combine: the video, its audio marker and frame exports share one dash-only counter; the returned path follows.
vprefix=prefix.replace('raw/image/','raw/video/',1)
names=[]
for _ in range(2):
 saved=video.DaSiWa_EnhancedVideoCombine().combine(images=torch.rand((4,16,16,3)),frame_rate=8,codec='VP9',container='WebM',bit_depth='8',quality=30,pingpong=False,
  save_metadata=True,filename_prefix=vprefix,save_output=True,pass_frames=False,audio={'waveform':torch.zeros((1,2,8000)),'sample_rate':16000},
  save_first_frame=True,extra_pnginfo={'deas_output_naming':True})
 main=saved['ui']['gifs'][0]['filename'];frame=saved['ui']['images'][0]['filename']
 assert main.endswith('-audio.webm') and '_' not in main and frame==main.removesuffix('.webm')+'-first-frame.png',(main,frame)
 assert Path(saved['result'][1])==out/'raw/video/2026-01-02'/main and Path(saved['result'][1]).is_file(),saved['result'][1]
 assert (out/'raw/video/2026-01-02'/frame).is_file() and main not in names
 names.append(main)
assert not list((out/'raw/video/2026-01-02').glob('*_*')),'underscore leftovers'
# Unmarked standalone saver calls retain their original naming.
plain=nodes.SaveImage().save_images(images=torch.zeros((1,8,8,3)),filename_prefix='standalone')
assert plain['ui']['images'][0]['filename'].endswith('_.png')
assert len(list((out/'raw/image/2026-01-02').glob('*.png')))>=4
print(json.dumps({'prefix':prefix,'saved':[str(p) for p in paths],'video':names},indent=2))
