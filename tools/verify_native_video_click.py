"""Real GTK/libmpv image-click regression (Linux/X11, debug build).
Usage: DISPLAY=:0 python tools/verify_native_video_click.py BINARY SCRATCH_OUT [VIDEO]
Only launches/terminates its own process. OUT must be new and inside Hermes scratch.
Uses the bundled splash (no App entry point), raw DOM controls and real native video.
Models NativeVideo click-to-IPC routing; does not verify React/Workbench integration.
No model/backend IPC is allowed; native_video_open uses software decoding.
"""
import ctypes
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import threading
import time


def main():
    assert sys.platform == 'linux', 'Linux/X11 only'
    binary, destination, *videos = sys.argv[1:]
    scratch = Path.home() / '.hermes/cache/scratch'
    out = Path(destination).resolve()
    assert out.is_relative_to(scratch.resolve()), 'OUT must be inside Hermes scratch'
    out.mkdir(parents=True, exist_ok=False)
    video = Path(videos[0]).resolve() if videos else out / 'moving-test.mp4'
    if not videos:
        subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i',
                        'testsrc2=size=640x360:rate=24,hue=h=60*t', '-t', '24', '-c:v', 'mpeg4',
                        '-threads', '1', '-q:v', '4', '-an', str(video)], check=True, timeout=30)
    assert video.is_file(), video
    script = r'''
(() => {
 // The bundled splash has no App entry point and never contacts backends/models.
 if(!location.pathname.endsWith('/splash.html')){location.replace('/splash.html');return;}
 const api=window.__TAURI_INTERNALS__, original=api.invoke.bind(api);
 const allowed=new Set(['native_video_open','native_video_update','native_startup_ready','plugin:event|emit','plugin:window|destroy']);
 let id, latest, root, domClicks=0;
 const invoke=(cmd,args,...rest)=>{
  if(!allowed.has(cmd))return Promise.reject(Error('Click probe blocks unrelated IPC: '+cmd));
  if(cmd==='native_video_open')id=args.id;
  return original(cmd,args,...rest);
 };
 const sleep=ms=>new Promise(r=>setTimeout(r,ms));
 const report=payload=>invoke('plugin:event|emit',{event:'deas-video-probe',payload});
 const assert=(ok,msg)=>{if(!ok)throw Error(msg);};
 const until=async(fn,msg)=>{for(let i=0;i<60;i++){if(fn())return;await sleep(100);}throw Error(msg);};
 const status=()=>invoke('native_video_update',{id,action:{kind:'status'}});
 const click=async(el,name,fraction=.5)=>{
  const r=el.getBoundingClientRect();
  assert(r.width>0&&r.height>0,'Invisible click target '+name);
  await report({phase:'click',name,point:[r.x+r.width*fraction,r.y+r.height/2],rect:{x:r.x,y:r.y,width:r.width,height:r.height},dpr:devicePixelRatio});
  await sleep(650);
 };
 document.addEventListener('DOMContentLoaded',()=>{(async()=>{
  try{
   localStorage.setItem('deas.locale.v1','en');
   document.querySelector('#root')?.setAttribute('style','display:none');
   const host=document.createElement('div');
   host.style.cssText='position:fixed;inset:0;background:#202020;z-index:2147483647;padding:80px;color:white;';
   document.body.append(host);
   await invoke('native_startup_ready');
   await sleep(1200);
   const mount=document.createElement('div');mount.style.width='640px';host.append(mount);
   {
    // shortcut: model NativeVideo's DOM click->IPC routing, not the Workbench UI.
    // Static markup only: no untrusted HTML is interpolated.
    mount.innerHTML='<div class="native-video-surface" style="width:640px;height:360px;background:black"></div><div style="display:flex;gap:16px;margin-top:12px"><button>Play/pause</button><input class="native-video-seek" type="range" min="0" max="24" step="any"><button>Mute</button></div>';
    const surface=mount.querySelector('.native-video-surface'), buttons=mount.querySelectorAll('button'),seek=mount.querySelector('input');
    surface.style.cssText='width:640px;height:360px;min-height:360px;background:black';
    const control=action=>invoke('native_video_update',{id,action});
    surface.onclick=buttons[0].onclick=()=>control({kind:'pause',paused:!latest.paused});
    buttons[1].onclick=()=>control({kind:'mute',muted:!latest.muted});
    seek.oninput=()=>control({kind:'seek',seconds:Number(seek.value)});
    await invoke('native_video_open',{id:crypto.randomUUID(),source:{path:VIDEO},autoPlay:true,repeat:true});
    const r=surface.getBoundingClientRect();
    await report({phase:'bounds',id,rect:{x:r.x,y:r.y,width:r.width,height:r.height},hidden:document.hidden});
    await control({kind:'bounds',x:r.x,y:r.y,width:r.width,height:r.height,visible:true});
    const timer=setInterval(async()=>{latest=await status();},100);
    root={unmount:()=>clearInterval(timer)};
    await report({phase:'mode',mode:'raw-native-same-DOM-routing'});
   }
   await invoke('native_startup_ready');
   await until(()=>latest?.width===640&&!latest.paused&&latest.position>.3,'Native video did not start');
   assert(!latest.error,latest.error);
   await sleep(1500); // Collect a rendered-frame sample before toolbar pause.
   const surface=mount.querySelector('.native-video-surface');
   surface.addEventListener('click',()=>domClicks++);
   const buttons=mount.querySelectorAll('button'), seek=mount.querySelector('.native-video-seek');
   assert(buttons.length===2&&!buttons[0].disabled&&!seek.disabled,'Toolbar controls missing/disabled');
   await click(buttons[0],'toolbar-pause');
   await until(()=>latest.paused,'Toolbar pause failed');
   const paused=await status();await sleep(600);const still=await status();
   assert(Math.abs(still.position-paused.position)<.15,'Paused toolbar clock advanced');
   await click(seek,'toolbar-seek',.35);
   await until(()=>latest.position>5,'Toolbar seek control failed');
   await click(buttons[1],'toolbar-mute');
   await until(()=>latest.muted,'Toolbar mute control failed');
   await click(buttons[1],'toolbar-unmute');
   await until(()=>!latest.muted,'Toolbar unmute control failed');
   await click(buttons[0],'toolbar-play');
   await until(()=>!latest.paused,'Toolbar play control failed');
   const before=await status();await sleep(500);const playing=await status();
   assert(playing.position>before.position+.15,'Toolbar play did not advance clock');
   await report({phase:'controls-passed',paused,still,before,playing});
   await click(surface,'image-pause');
   let imagePaused=await status();
   await report({phase:'image-pause-result',before:playing,after:imagePaused,domClicks});
   assert(imagePaused.paused,'Real GTK image click did not pause playback');
   await sleep(600);const imageStill=await status();
   assert(Math.abs(imageStill.position-imagePaused.position)<.15,'Image pause clock advanced');
   await click(surface,'image-resume');
   await until(()=>!latest.paused,'Real GTK image click did not resume playback');
   const resumed=await status();await sleep(700);const advanced=await status();
   assert(advanced.position>resumed.position+.2,'Image resume did not advance clock');
   await report({phase:'passed',imagePaused,imageStill,resumed,advanced,domClicks});
  }catch(e){await report({phase:'failed',error:String(e),latest,domClicks,location:location.href});}
  finally{root?.unmount();if(id)await invoke('native_video_update',{id,action:{kind:'close'}}).catch(()=>{});await invoke('plugin:window|destroy',{label:'main'});}
 })();},{once:true});
})();
'''.replace('VIDEO', json.dumps(str(video)))
    (out / 'probe.js').write_text(script, encoding='utf-8')
    env = dict(os.environ, GDK_BACKEND='x11', DEAS_VIDEO_TEST_SCRIPT=str(out / 'probe.js'),
               DEAS_VIDEO_PROBE='1', LIBGL_ALWAYS_SOFTWARE='1', WEBKIT_DISABLE_DMABUF_RENDERER='1',
               XDG_CONFIG_HOME=str(out / 'config'), XDG_DATA_HOME=str(out / 'data'),
               XDG_CACHE_HOME=str(out / 'cache'))
    x = ctypes.CDLL('libX11.so.6')
    t = ctypes.CDLL('libXtst.so.6')
    x.XOpenDisplay.restype = ctypes.c_void_p
    d = x.XOpenDisplay(None)
    assert d, 'X11 display unavailable'
    x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x.XDefaultRootWindow.restype = ctypes.c_ulong
    x.XTranslateCoordinates.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong)]
    class ClientMessage(ctypes.Structure):
        _fields_ = [('type', ctypes.c_int), ('serial', ctypes.c_ulong), ('send_event', ctypes.c_int), ('display', ctypes.c_void_p), ('window', ctypes.c_ulong), ('message_type', ctypes.c_ulong), ('format', ctypes.c_int), ('data', ctypes.c_long * 5)]
    class XEvent(ctypes.Union):
        _fields_ = [('message', ClientMessage), ('pad', ctypes.c_long * 24)]
    x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x.XInternAtom.restype = ctypes.c_ulong
    x.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(XEvent)]
    x.XFlush.argtypes = [ctypes.c_void_p]
    x.XCloseDisplay.argtypes = [ctypes.c_void_p]
    t.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
    t.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    p = subprocess.Popen([str(Path(binary).resolve())], env=env, stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT, text=True, start_new_session=True)
    def stop():
        try:
            os.killpg(p.pid, signal.SIGTERM)
            p.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.wait(timeout=3)
        except ProcessLookupError:
            pass
    timer = threading.Timer(55, stop)
    timer.start()
    reports, lines = [], []
    code = None
    try:
        assert p.stdout is not None
        for line in p.stdout:
            lines.append(line)
            if 'DEAS_VIDEO_PROBE ' not in line:
                continue
            result = json.loads(line.split('DEAS_VIDEO_PROBE ', 1)[1])
            reports.append(result)
            if result['phase'] != 'click':
                continue
            windows = re.findall(r'0x[0-9a-f]+', subprocess.check_output(['xprop', '-root', '_NET_CLIENT_LIST'], env=env, text=True, timeout=3))
            owned = [w for w in windows if f'= {p.pid}' in subprocess.check_output(['xprop', '-id', w, '_NET_WM_PID'], env=env, text=True, timeout=3)]
            # The startup splash can share the PID: select the main window by title.
            owned = [w for w in owned if 'Dark Engine AI Studio' in subprocess.check_output(['xprop', '-id', w, '_NET_WM_NAME'], env=env, text=True, timeout=3)]
            assert len(owned) == 1, owned
            wid = int(owned[0], 16)
            event = XEvent()
            event.message = ClientMessage(33, 0, 1, d, wid, x.XInternAtom(d, b'_NET_ACTIVE_WINDOW', 0), 32, (ctypes.c_long * 5)(2, 0, 0, 0, 0))
            x.XSendEvent(d, x.XDefaultRootWindow(d), 0, (1 << 20) | (1 << 19), ctypes.byref(event))
            x.XFlush(d)
            time.sleep(.2)
            ox, oy, child = ctypes.c_int(), ctypes.c_int(), ctypes.c_ulong()
            assert x.XTranslateCoordinates(d, wid, x.XDefaultRootWindow(d), 0, 0, ctypes.byref(ox), ctypes.byref(oy), ctypes.byref(child))
            px, py = result['point']
            scale = result['dpr']
            t.XTestFakeMotionEvent(d, -1, round(ox.value + px*scale), round(oy.value + py*scale), 0)
            t.XTestFakeButtonEvent(d, 1, 1, 0)
            x.XFlush(d)
            time.sleep(.07)
            t.XTestFakeButtonEvent(d, 1, 0, 0)
            x.XFlush(d)
        code = p.wait(timeout=5)
    finally:
        timer.cancel()
        stop()
        x.XCloseDisplay(d)
        (out / 'native.log').write_text(''.join(lines), encoding='utf-8')
        (out / 'results.json').write_text(json.dumps(reports, indent=2), encoding='utf-8')
    frames = re.findall(r'DEAS_VIDEO_FRAME .* hash=(\d+) range=(\d+) gl_error=(\d+)', ''.join(lines))
    summary = {'passed': code == 0 and any(r['phase'] == 'passed' for r in reports),
               'exitCode': code, 'controlsPassed': any(r['phase'] == 'controls-passed' for r in reports),
               'distinctFrames': len({f[0] for f in frames}), 'output': str(out)}
    summary['passed'] = summary['passed'] and summary['distinctFrames'] >= 2 and all(f[2] == '0' for f in frames)
    (out / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    print(json.dumps(summary))
    if not summary['passed']:
        print(json.dumps([r for r in reports if r['phase'] == 'failed']))
        raise SystemExit(1)


if __name__ == '__main__':
    main()
