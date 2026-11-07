#!/usr/bin/env python3
"""Real Linux References modal/crop probe, using tiny model-header fixtures, not inference.
Usage: python tools/verify_native_references.py DEBUG_BINARY_OR_DEV_LAUNCHER OUTPUT_DIR VIDEO [OWNER_FLAC_AUDIO]
Requires the owned Vite dev frontend (tools/inpaint-fixture.html); source video stays read-only.
"""
import json
import os
from pathlib import Path
import re
import shutil
import signal
import struct
import subprocess
import sys
from urllib.parse import urlencode

if sys.platform != 'linux':
    sys.exit('Linux-only native References probe; not Windows acceptance.')
binary, output, video, *audio_files = sys.argv[1:]
if len(audio_files)>1:
    sys.exit('Pass at most one optional audio-reference fixture.')
output = Path(output).resolve()
output.mkdir(parents=True, exist_ok=False)
root = output / 'fixture-environment'
models = root / 'ComfyUI/models'
# Real bounded safetensors fixtures: enough for architecture discovery, never loaded for inference.
for category, name, tensors in [
    ('diffusion_models', 'h3.safetensors', ['video.patch.proj', 'audio.patch.proj']),
    ('text_encoders', 'minimax_h3.safetensors', ['fixture']),
    ('vae', 'h3_video_vae.safetensors', ['fixture']),
    ('vae', 'h3_audio_vae.safetensors', ['fixture'])
]:
    path = models / category / name
    path.parent.mkdir(parents=True, exist_ok=True)
    header = json.dumps({key: {'dtype': 'F32', 'shape': [1], 'data_offsets': [i*4, (i+1)*4]} for i, key in enumerate(tensors)}).encode()
    path.write_bytes(struct.pack('<Q', len(header)) + header + bytes(4*len(tensors)))
source = root / 'ComfyUI/input/clip.mp4'
source.parent.mkdir(parents=True, exist_ok=True)
shutil.copyfile(Path(video).resolve(), source)
for name in ['b.mp4', 'c.mp4']:
    shutil.copyfile(source, source.with_name(name))
audio_name = ''
if audio_files:
    audio_name = 'reference' + Path(audio_files[0]).suffix
    shutil.copyfile(Path(audio_files[0]).resolve(), source.with_name(audio_name))
(root / 'ComfyUI/output/raw').mkdir(parents=True)
url = '/tools/inpaint-fixture.html?' + urlencode({'root': str(root), 'endpoint': 'http://127.0.0.1:1'})
script = r'''
(()=>{
 if(!location.pathname.includes('/tools/inpaint-fixture.html')){location.replace(FIXTURE_URL);return;}
 localStorage.clear();localStorage.setItem('deas.locale.v1','en');
 localStorage.setItem('deas.pref.studio.run.pipeline',JSON.stringify('ref2v'));
 const call=(cmd,args={})=>window.__TAURI_INTERNALS__.invoke(cmd,args);
 const sleep=ms=>new Promise(r=>setTimeout(r,ms));
 const until=async(fn,name)=>{for(let n=0;n<240;n++){const v=fn();if(v)return v;await sleep(50);}throw Error('Timeout '+name);};
 const assert=(ok,name)=>{if(!ok)throw Error(name);};
 const report=payload=>call('plugin:event|emit',{event:'deas-video-probe',payload});
 const input=(el,value)=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,value);el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}));};
 (async()=>{
  try{
   const opener=await until(()=>document.querySelector('[data-open-references]'),'opener');opener.click();
   const dialog=await until(()=>document.querySelector('.references-overlay:modal'),'modal');
   const inputFolder=Array.from(dialog.querySelectorAll('.references-picker button')).find(b=>b.textContent==='Input');assert(inputFolder,'input folder');inputFolder.click();
   if(AUDIO_NAME){
    const audioTile=await until(()=>dialog.querySelector('.references-picker button[title="'+AUDIO_NAME+'"]'),'audio picker');audioTile.click();
    const audioCard=await until(()=>dialog.querySelector('[data-ref="'+AUDIO_NAME+'"]'),'audio reference');
    const edit=audioCard.querySelector('[data-edit-reference]');edit.click();
    let editor=await until(()=>dialog.querySelector('.reference-editor-dialog:modal'),'audio editor');
    const duration=()=>Number(editor.querySelector('.native-video-seek')?.max||0);
    const position=()=>Number(editor.querySelector('.native-video-seek')?.value||0);
    await until(()=>duration()>0,'FLAC duration');
    assert(Math.abs(duration()-27)<0.01,'owner FLAC duration');
    assert(editor.querySelector('[data-audio-only]')&&!editor.querySelector('audio,video'),'native audio-only player');
    const fields=editor.querySelectorAll('input[type=number]');input(fields[0],'1');await sleep(100);input(fields[1],'3');await sleep(100);
    editor.querySelector('.reference-media-actions button').click();
    await until(()=>position()>1.1,'FLAC crop advances');
    await until(()=>position()>2.95&&position()<3.1&&editor.querySelector('.native-video-controls button')?.getAttribute('aria-label')==='Play','FLAC crop stops');
    const crop=position();editor.dispatchEvent(new Event('cancel',{cancelable:true}));await sleep(100);
    edit.click();editor=await until(()=>dialog.querySelector('.reference-editor-dialog:modal'),'FLAC reopen');
    const kept=editor.querySelectorAll('input[type=number]');assert(kept[0].value==='1'&&kept[1].value==='3','FLAC trims retained');
    assert(!editor.querySelector('.error-message'),'FLAC reference has no load error');
    await report({phase:'passed',audio:AUDIO_NAME,duration:27,crop});
    await call('plugin:window|destroy',{label:'main'});return;
   }
   const tile=await until(()=>dialog.querySelector('.references-picker button[title="clip.mp4"]'),'picker');tile.click();
   let card=await until(()=>dialog.querySelector('[data-ref="clip.mp4"]'),'reference');
   assert(!dialog.querySelector('[data-ref] video[controls], [data-ref] audio, [data-ref] [data-native-video]'),'overview has no players');
   let edit=card.querySelector('[data-edit-reference]');edit.focus();edit.click();
   let editor=await until(()=>dialog.querySelector('.reference-editor-dialog:modal'),'nested editor');
   assert(editor.contains(document.activeElement),'editor focus');opener.focus();assert(editor.contains(document.activeElement),'modal blocks background focus');
   const duration=()=>Number(editor.querySelector('.native-video-seek')?.max||0);
   const position=()=>Number(editor.querySelector('.native-video-seek')?.value||0);
   const paused=()=>editor.querySelector('.native-video-controls button')?.getAttribute('aria-label')==='Play';
   await until(()=>duration()>0,'metadata');
   const surface=editor.querySelector('.native-video-surface');
   await until(()=>{const r=surface.getBoundingClientRect();return [[r.left+1,r.top+1],[r.right-1,r.top+1],[r.left+1,r.bottom-1],[r.right-1,r.bottom-1]].every(([x,y])=>surface.contains(document.elementFromPoint(x,y)));},'unoccluded native surface');
   assert(editor.querySelector('[data-native-video]')&&!editor.querySelector('video'),'native reuse');
   let fields=editor.querySelectorAll('input[type=number]');input(fields[0],'1');await sleep(100);input(fields[1],'5');await sleep(100);
   const play=editor.querySelector('.reference-media-actions button');assert(!play.disabled,'crop enabled');play.click();
   await until(()=>position()>1.1&&!paused(),'playing');
   await until(()=>position()>4.9&&position()<5.1&&paused(),'crop stop');const crop=position();
   assert(editor.querySelector('[aria-label=Unmute]')?.disabled,'video-only forced mute');
   // Dispatch Escape's cancel event; browser acceptance drives real keyboard traversal.
   editor.dispatchEvent(new Event('cancel',{cancelable:true}));await sleep(100);
   assert(!dialog.querySelector('.reference-editor-dialog')&&dialog.matches(':modal'),'cancel closes only editor');assert(document.activeElement===edit,'editor focus return');
   edit.click();editor=await until(()=>dialog.querySelector('.reference-editor-dialog:modal'),'reopened editor');
   fields=editor.querySelectorAll('input[type=number]');assert(fields[0].value==='1'&&fields[1].value==='5','reopened crop');
   const mountedEditor=editor;
   const mode=editor.querySelector('select');Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(mode,'audio');mode.dispatchEvent(new Event('change',{bubbles:true}));
   await sleep(300);assert(editor===mountedEditor&&editor.isConnected&&editor.matches(':modal'),'mode keeps editor mounted');card=dialog.querySelector('[data-ref="clip.mp4"]');assert(editor.querySelectorAll('input[type=number]')[0].value==='1'&&editor.querySelectorAll('input[type=number]')[1].value==='5','mode keeps crop');await until(()=>duration()>0,'audio metadata');
   assert(card.closest('section')===dialog.querySelectorAll('.wb-lane')[2],'audio mode changes lane');
   assert(editor.querySelector('.native-video-surface').style.display==='none','audio hides picture');
   editor.querySelector('.reference-media-actions button').click();
   await until(()=>position()>1.1&&!paused(),'audio playing');
   await until(()=>position()>4.9&&position()<5.1&&paused(),'audio crop stop');const audioCrop=position();
   assert(editor.querySelector('[aria-label=Mute]'),'audio-only unmuted');
   editor.dispatchEvent(new Event('cancel',{cancelable:true}));await sleep(100);assert(dialog.matches(':modal')&&!dialog.querySelector('.reference-editor-dialog'),'audio editor closed');
   // Add B after opening, then native-drop C: neither B nor A's current trims may disappear.
   dialog.querySelector('.references-picker button[title="b.mp4"]').click();
   await until(()=>dialog.querySelectorAll('[data-ref]').length===2,'second reference');
   const area=dialog.querySelector('.references-picker .media-library').getBoundingClientRect();
   await call('plugin:event|emit',{event:'tauri://drag-drop',payload:{paths:[EXTRA_SOURCE],position:{x:(area.left+20)*devicePixelRatio,y:(area.top+20)*devicePixelRatio}}});
   await until(()=>dialog.querySelectorAll('[data-ref]').length===3,'dropped third reference');
   edit=dialog.querySelector('[data-ref="clip.mp4"] [data-edit-reference]');edit.focus();edit.click();
   editor=await until(()=>dialog.querySelector('.reference-editor-dialog:modal'),'drop crop editor');
   const kept=editor.querySelectorAll('input[type=number]');
   assert(kept[0].value==='1'&&kept[1].value==='5','drop preserved existing trims');
   editor.dispatchEvent(new Event('cancel',{cancelable:true}));await sleep(100);assert(document.activeElement===edit,'drop editor focus return');
   assert(!dialog.querySelector('[data-ref] video[controls], [data-ref] audio, [data-ref] [data-native-video]'),'overview still has no players');
   assert(dialog.querySelector('[data-ref="b.mp4"]'),'drop preserved second reference');

   dialog.querySelector('header button').click();await sleep(200);
   assert(!document.querySelector('.references-overlay'),'close');assert(document.activeElement===opener,'focus return');
   await report({phase:'passed',crop,audioCrop,nativeReferencesModal:true,nestedEditor:true,editorSurvivesLaneChange:true,overviewPlayers:0});
  }catch(e){await report({phase:'failed',error:String(e),href:location.href,body:document.body.innerText.slice(0,5000)});}
  await call('plugin:window|destroy',{label:'main'});
 })();
})();
'''.replace('FIXTURE_URL', json.dumps(url)).replace('EXTRA_SOURCE', json.dumps(str(source.with_name('c.mp4')))).replace('AUDIO_NAME', json.dumps(audio_name))
(output / 'probe.js').write_text(script)
env = dict(os.environ, DEAS_VIDEO_TEST_SCRIPT=str(output / 'probe.js'), DEAS_VIDEO_PROBE='1', XDG_CONFIG_HOME=str(output / 'config'), XDG_DATA_HOME=str(output / 'data'), XDG_CACHE_HOME=str(output / 'cache'))
with (output / 'native.log').open('w') as log:
    process = subprocess.Popen([str(Path(binary).resolve())], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        returncode = process.wait(timeout=180)
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()

text = (output / 'native.log').read_text()
reports = [json.loads(line.split('DEAS_VIDEO_PROBE ', 1)[1]) for line in text.splitlines() if 'DEAS_VIDEO_PROBE ' in line]
frames = re.findall(r'DEAS_VIDEO_FRAME id=([^ ]+) hash=(\d+) range=(\d+) gl_error=(\d+) position=([^\n]+)', text)
assert returncode == 0 and any(r.get('phase') == 'passed' for r in reports), text[-8000:]
if not audio_files:
    assert len({f[1] for f in frames}) >= 2 and all(f[3] == '0' for f in frames), frames
(output / 'results.json').write_text(json.dumps({'reports': reports, 'framebuffer_samples': frames}, indent=2))
print(json.dumps({'passed': True, 'nativeReferencesModal': True, 'framebuffer_samples': len(frames), 'output': str(output)}))
