#!/usr/bin/env python3
"""Run the real DEAS debug binary on the current desktop with isolated config/data.
Usage: python tools/verify_native_video.py BINARY OUTPUT_DIR VIDEO [VIDEO ...]
No model/backend/service is started. Requires DEAS debug probe hook; originals read-only.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys

if sys.platform != 'linux':
    sys.exit('Linux-only native video/framebuffer diagnostic; not Windows acceptance.')

binary, output, *files = sys.argv[1:]
output = Path(output).resolve()
output.mkdir(parents=True, exist_ok=False)
script = r'''
(async()=>{
 const sleep=ms=>new Promise(r=>setTimeout(r,ms));
 if(FIXTURE_URL&&!location.pathname.includes('/tools/inpaint-fixture.html')){location.replace(FIXTURE_URL);return;}
 await sleep(2000);
 const timings=[];
 const call=async(cmd,args={})=>{const start=performance.now();const value=await window.__TAURI_INTERNALS__.invoke(cmd,args);if(cmd==='native_video_update')timings.push({kind:args.action.kind,ms:performance.now()-start});return value;};
 const report=payload=>call('plugin:event|emit',{event:'deas-video-probe',payload});
 const assert=(ok,message)=>{if(!ok)throw Error(message);};
 const until=async(fn,label)=>{for(let n=0;n<160;n++){const v=await fn();if(v)return v;await sleep(100);}throw Error('Timeout '+label);};
 try {
  await report({phase:'ready'});
  document.querySelector('[data-studio-tab="generate"]')?.click();
  await sleep(400);
  document.querySelector('[data-dock-tab="media"]').click();
  await sleep(400);
  const picker=await until(()=>document.querySelector('#studio-dock .media-library'),'Media picker');
  const drop=picker.getBoundingClientRect();
  await call('plugin:event|emit',{event:'tauri://drag-drop',payload:{paths:[FILES[0]],position:{x:(drop.left+20)*devicePixelRatio,y:(drop.top+20)*devicePixelRatio}}});
  await until(()=>document.querySelector('[data-native-video]'),'Workbench preview');
  const play=await until(()=>{const b=document.querySelector('.native-video-controls button');return b&&!b.disabled&&Number(document.querySelector('.native-video-seek')?.max)>0?b:null;},'Workbench decoded');
  play.click();await sleep(2200);
  await report({phase:'workbench',html:document.querySelector('[data-native-video]').outerHTML});
  const player=document.querySelector('[data-native-video]');
  const metadata=document.querySelector('[data-metadata-toggle]');
  metadata.click();
  const geometry=await until(()=>{
   const surface=player.querySelector('.native-video-surface'),panel=document.querySelector('.wb-stage > .wb-metadata');
   if(!surface||!panel)return null;
   const video=surface.getBoundingClientRect(),meta=panel.getBoundingClientRect();
   return video.width>0&&video.right<meta.left?{video:{left:video.left,right:video.right,top:video.top,bottom:video.bottom},metadata:{left:meta.left,right:meta.right}}:null;
  },'non-overlapping native video metadata');
  await sleep(1200);
  assert(document.querySelector('[data-native-video]')===player,'Metadata remounted the native player');
  await report({phase:'metadata',geometry,html:player.outerHTML});
  const more=document.querySelector('[data-more]');
  assert(more,'Missing video More control');more.click();
  await until(()=>more.closest('details').open,'More open');
  more.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));await sleep(100);
  assert(!more.closest('details').open&&document.activeElement===more,'More Escape/focus');
  metadata.click();await sleep(200);
  assert(document.querySelector('[data-native-video]')===player,'Closing Metadata remounted the player');
  assert(!document.querySelector('video'),'Linux created a WebKit decoder');
  document.querySelector('[data-preview-close]').click();await sleep(300);
  for(const path of FILES) {
   const id=crypto.randomUUID();const update=action=>call('native_video_update',{id,action});
   await call('native_video_open',{id,source:{path},autoPlay:false,repeat:false});
   await update({kind:'bounds',x:100,y:150,width:640,height:360,visible:true});
   const a=await until(async()=>{const s=await update({kind:'status'});assert(!s.error,s.error);return s.duration>0&&s.width>0?s:null;},'decoded');
   await update({kind:'pause',paused:false});
   const b=await until(async()=>{const s=await update({kind:'status'});return s.position>a.position+0.3?s:null;},'advancing');
   await sleep(1300);
   await update({kind:'pause',paused:true});await sleep(250);const paused=await update({kind:'status'});assert(paused.paused,'pause');
   await update({kind:'seek',seconds:Math.min(1,paused.duration/2)});await sleep(400);const seek=await update({kind:'status'});assert(Math.abs(seek.position-Math.min(1,paused.duration/2))<0.25,'seek');
   await update({kind:'mute',muted:true});await update({kind:'volume',volume:35});await sleep(100);const audio=await update({kind:'status'});assert(audio.muted&&audio.volume===35,'audio controls');
   await update({kind:'bounds',x:80,y:120,width:480,height:270,visible:true});await update({kind:'pause',paused:false});await sleep(1200);
   await update({kind:'bounds',x:80,y:120,width:480,height:270,visible:false});await update({kind:'close'});await update({kind:'close'});
   await report({phase:'clip',path,a,b,paused,seek,audio});
  }
  const logs=await call('native_logs',{after:0});await report({phase:'logs',logs:logs.filter(e=>e.text.includes('NativeVideo'))});
  await report({phase:'timings',timings});
  assert(timings.filter(t=>t.kind==='status'||t.kind==='pause').every(t=>t.ms<750),'Unresponsive playback controls');
  await report({phase:'passed'});
 }catch(e){await report({phase:'failed',error:String(e)});}
 await call('plugin:window|destroy',{label:'main'});
})();
'''.replace('FILES', json.dumps([str(Path(p).resolve()) for p in files])).replace('FIXTURE_URL', json.dumps(os.environ.get('DEAS_VIDEO_FIXTURE_URL', '')))
(output / 'probe.js').write_text(script, encoding='utf-8')
env = dict(os.environ, DEAS_VIDEO_TEST_SCRIPT=str(output / 'probe.js'), DEAS_VIDEO_PROBE='1', XDG_CONFIG_HOME=str(output / 'config'), XDG_DATA_HOME=str(output / 'data'), XDG_CACHE_HOME=str(output / 'cache'))
with (output / 'native.log').open('w') as log:
    result = subprocess.run([str(Path(binary).resolve())], env=env, stdout=log, stderr=subprocess.STDOUT, timeout=90)
text = (output / 'native.log').read_text(encoding='utf-8')
reports = [json.loads(line.split('DEAS_VIDEO_PROBE ', 1)[1]) for line in text.splitlines() if 'DEAS_VIDEO_PROBE ' in line]
frames = re.findall(r'DEAS_VIDEO_FRAME id=([^ ]+) hash=(\d+) range=(\d+) gl_error=(\d+) position=([^\n]+)', text)
assert result.returncode == 0, text[-6000:]
assert any(r.get('phase') == 'passed' for r in reports), text[-6000:]
for clip in [r for r in reports if r.get('phase') == 'clip']:
    assert clip['b']['position'] > clip['a']['position']
assert len({f[1] for f in frames}) >= 2 and all(f[3] == '0' for f in frames), frames
(output / 'results.json').write_text(json.dumps({'reports':reports,'framebuffer_samples':frames}, indent=2), encoding='utf-8')
print(json.dumps({'passed':True,'clips':len(files),'framebuffer_samples':len(frames),'output':str(output)}))
