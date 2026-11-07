import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { ReferenceMedia } from './ReferenceMedia';
import type { VideoRef } from './ReferencesPanel';
import { invoke, isTauri } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core',()=>({isTauri:vi.fn(()=>false),invoke:vi.fn().mockResolvedValue(undefined)}));

vi.mock('../../i18n/I18nProvider',()=>({useI18n:()=>({t:(key:string)=>key})}));
const key=(name:string)=>`studio.run.video.preview.${name}`;
const reference=(x:Partial<VideoRef>={}):VideoRef=>({item:{name:'clip.mp4',src:'asset://clip.mp4',media:'video'},second:0,strength:1,mediaMode:'video',trimStart:2,trimEnd:5,...x});
let play:ReturnType<typeof vi.spyOn>,pause:ReturnType<typeof vi.spyOn>;
beforeEach(()=>{vi.useFakeTimers();play=vi.spyOn(HTMLMediaElement.prototype,'play').mockResolvedValue();pause=vi.spyOn(HTMLMediaElement.prototype,'pause').mockImplementation(()=>{});});
afterEach(()=>{cleanup();vi.restoreAllMocks();vi.useRealTimers();});
function mount(r=reference()){
 const onChange=vi.fn();const view=render(<ReferenceMedia reference={r} onChange={onChange}/>);
 const media=()=>view.container.querySelector('video,audio') as HTMLMediaElement;
 const metadata=(duration=20)=>{Object.defineProperty(media(),'duration',{configurable:true,value:duration});fireEvent.loadedMetadata(media());};
 return {...view,onChange,media,metadata};
}

test('crop playback seeks to start and stops exactly at end without timeupdate',async()=>{
 const {media,metadata}=mount();metadata();
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 expect(media().currentTime).toBe(2);expect(play).toHaveBeenCalledOnce();
 act(()=>vi.advanceTimersByTime(2999));expect(pause).not.toHaveBeenCalled();
 act(()=>vi.advanceTimersByTime(1));expect(pause).toHaveBeenCalledOnce();expect(media().currentTime).toBe(5);
 expect(screen.getByRole('button',{name:key('play')})).toBeTruthy();
});

test('crop clamps seeking, reschedules for playback rate and also stops on timeupdate',async()=>{
 const {media,metadata}=mount();metadata();
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 media().currentTime=0;fireEvent.seeking(media());expect(media().currentTime).toBe(2);
 media().currentTime=4;media().playbackRate=2;fireEvent.rateChange(media());
 act(()=>vi.advanceTimersByTime(499));expect(pause).not.toHaveBeenCalled();
 act(()=>vi.advanceTimersByTime(1));expect(media().currentTime).toBe(5);
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 media().currentTime=6;fireEvent.timeUpdate(media());expect(media().currentTime).toBe(5);expect(pause).toHaveBeenCalledTimes(2);
});

test('toggle and native pause cancel the crop timer; waiting does not consume the crop',async()=>{
 const {media,metadata}=mount();metadata();
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 fireEvent.waiting(media());act(()=>vi.advanceTimersByTime(5000));expect(pause).not.toHaveBeenCalled();
 fireEvent.playing(media());fireEvent.click(screen.getByRole('button',{name:key('pause')}));
 act(()=>vi.advanceTimersByTime(5000));expect(pause).toHaveBeenCalledOnce();
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 fireEvent.pause(media());act(()=>vi.advanceTimersByTime(5000));expect(media().currentTime).toBe(2);
});

test('renders full images, muted video, both streams and sound-only native audio',()=>{
 const {container,rerender,onChange}=mount();
 expect(container.querySelector('video')!.muted).toBe(true);expect(container.querySelector('video')!.controls).toBe(true);
 fireEvent.change(screen.getByRole('combobox'),{target:{value:'video_audio'}});expect(onChange).toHaveBeenCalledWith({mediaMode:'video_audio'});
 rerender(<ReferenceMedia reference={reference({mediaMode:'video_audio'})} onChange={onChange}/>);
 expect(container.querySelector('video')!.muted).toBe(false);
 rerender(<ReferenceMedia reference={reference({mediaMode:'audio'})} onChange={onChange}/>);
 expect(container.querySelector('video')).toBeNull();expect(container.querySelector('audio')!.controls).toBe(true);
 rerender(<ReferenceMedia reference={reference({item:{name:'song.wav',src:'asset://song.wav',media:'audio'}})} onChange={onChange}/>);
 expect(container.querySelector('audio')).toBeTruthy();expect(screen.queryByRole('combobox')).toBeNull();
 rerender(<ReferenceMedia reference={reference({item:{name:'face.png',src:'asset://face.png',media:'image'}})} onChange={onChange}/>);
 expect(screen.getByRole('img').getAttribute('src')).toBe('asset://face.png');expect(container.querySelector('audio,video,input')).toBeNull();
 fireEvent.error(screen.getByRole('img'));expect(screen.getByRole('alert').textContent).toBe(key('loadError'));
});

test('edits numeric/range trims, uses playhead, resets, and never rewrites trims from metadata',()=>{
 const {metadata,media,onChange}=mount();metadata();expect(onChange).toHaveBeenCalledExactlyOnceWith({sourceDuration:20});onChange.mockClear();
 fireEvent.change(screen.getByRole('spinbutton',{name:'studio.run.video.trimStartOf'}),{target:{value:'3.25'}});expect(onChange).toHaveBeenLastCalledWith({trimStart:3.25});
 fireEvent.change(screen.getByRole('slider',{name:key('trimEndRange')}),{target:{value:'8'}});expect(onChange).toHaveBeenLastCalledWith({trimEnd:8});
 fireEvent.change(screen.getByRole('spinbutton',{name:'studio.run.video.trimEndOf'}),{target:{value:'9999'}});expect(onChange).toHaveBeenLastCalledWith({trimEnd:20});
 media().currentTime=4.2;fireEvent.timeUpdate(media());
 fireEvent.click(screen.getByRole('button',{name:key('setIn')}));expect(onChange).toHaveBeenLastCalledWith({trimStart:4.2});
 fireEvent.click(screen.getByRole('button',{name:key('setOut')}));expect(onChange).toHaveBeenLastCalledWith({trimEnd:4.2});
 fireEvent.click(screen.getByRole('button',{name:key('reset')}));expect(onChange).toHaveBeenLastCalledWith({trimStart:undefined,trimEnd:undefined});
 fireEvent.change(screen.getByRole('spinbutton',{name:'studio.run.video.trimEndOf'}),{target:{value:''}});expect(onChange).toHaveBeenLastCalledWith({trimEnd:undefined});
});

test('unknown/nonfinite duration and invalid bounds disable bounded controls without replacing saved trims',()=>{
 const {metadata,onChange}=mount(reference({trimStart:30,trimEnd:35}));
 expect((screen.getByRole('button',{name:key('play')}) as HTMLButtonElement).disabled).toBe(true);
 expect((screen.getByRole('button',{name:key('setIn')}) as HTMLButtonElement).disabled).toBe(true);
 for(const n of [Infinity,NaN,0]){metadata(n);expect((screen.getByRole('slider',{name:key('trimEndRange')}) as HTMLInputElement).disabled).toBe(true);}
 metadata(20);expect((screen.getByRole('button',{name:key('play')}) as HTMLButtonElement).disabled).toBe(true);
 expect((screen.getByRole('spinbutton',{name:'studio.run.video.trimEndOf'}) as HTMLInputElement).value).toBe('35');
 expect(onChange).toHaveBeenCalledExactlyOnceWith({sourceDuration:20});
});

test('load and play failures are visible and cancel timers',async()=>{
 const {metadata,media}=mount();metadata();play.mockRejectedValueOnce(new Error('private file path'));
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 expect(screen.getByRole('alert').textContent).toBe(key('playError'));
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 fireEvent.error(media());expect(screen.getByRole('alert').textContent).toBe(key('loadError'));
 const count=pause.mock.calls.length;act(()=>vi.advanceTimersByTime(10000));expect(pause).toHaveBeenCalledTimes(count);
});

test('source/mode changes and unmount stop playback and remove timers',async()=>{
 const {metadata,rerender,onChange,unmount}=mount();metadata();
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 rerender(<ReferenceMedia reference={reference({mediaMode:'audio'})} onChange={onChange}/>);
 expect(pause).toHaveBeenCalledOnce();expect(vi.getTimerCount()).toBe(0);
 metadata();await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 rerender(<ReferenceMedia reference={reference({item:{name:'new.mp4',src:'asset://new.mp4',media:'video'}})} onChange={onChange}/>);
 expect(pause).toHaveBeenCalledTimes(2);expect(vi.getTimerCount()).toBe(0);
 metadata();await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 unmount();expect(pause).toHaveBeenCalledTimes(3);expect(vi.getTimerCount()).toBe(0);
});

test('a late play resolution cannot restart playback after unmount',async()=>{
 let resolve!:()=>void;play.mockReturnValueOnce(new Promise<void>(r=>{resolve=r;}));
 const {metadata,unmount}=mount();metadata();fireEvent.click(screen.getByRole('button',{name:key('play')}));
 unmount();pause.mockClear();await act(async()=>resolve());expect(pause).toHaveBeenCalledOnce();expect(vi.getTimerCount()).toBe(0);
});

test('native preview errors reach the fixed path-free shared log once per player, never in browser mode',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockClear();
 const {metadata,media}=mount();metadata();fireEvent.error(media());fireEvent.error(media());
 play.mockRejectedValueOnce(new Error('sensitive local path'));
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 expect(invoke).toHaveBeenCalledExactlyOnceWith('native_logs_reference_media_error');
 vi.mocked(isTauri).mockReturnValue(false);cleanup();vi.mocked(invoke).mockClear();
 const browser=mount();fireEvent.error(browser.media());expect(invoke).not.toHaveBeenCalled();
});

test('trim edits stop the current crop without replacing metadata or saved bounds',async()=>{
 const {metadata,rerender,onChange}=mount();metadata();onChange.mockClear();
 await act(async()=>fireEvent.click(screen.getByRole('button',{name:key('play')})));
 rerender(<ReferenceMedia reference={reference({trimEnd:8})} onChange={onChange}/>);
 expect(pause).toHaveBeenCalledOnce();expect(vi.getTimerCount()).toBe(0);expect(onChange).not.toHaveBeenCalled();
});

const flac=()=>reference({item:{name:'song.flac',path:'/starred/audio/song.flac',src:'asset://song.flac',media:'audio'},mediaMode:'audio'});
function linuxAudio(){
 vi.mocked(isTauri).mockReturnValue(true);
 vi.spyOn(navigator,'userAgent','get').mockReturnValue('Linux DEAS');
 let status={position:2,duration:27,width:0,height:0,paused:true,muted:false,volume:100,error:undefined as string|undefined};
 vi.mocked(invoke).mockClear().mockImplementation(async(command,args)=>command==='native_video_update'&&(args as {action:{kind:string}}).action.kind==='status'?status:undefined);
 const update=async(next:Partial<typeof status>)=>{status={...status,...next};await act(async()=>{vi.advanceTimersByTime(150);});};
 return {update};
}

test('Linux FLAC uses unmuted native audio controls with duration and crop transport',async()=>{
 const {update}=linuxAudio();const {container,onChange}=mount(flac());await act(async()=>{});
 expect(container.querySelector('audio,video')).toBeNull();
 expect(container.querySelector('[data-native-video][data-audio-only]')).toBeTruthy();
 expect(container.querySelector<HTMLElement>('.native-video-surface')!.style.display).toBe('none');
 expect(invoke).toHaveBeenCalledWith('native_video_open',expect.objectContaining({source:{path:'/starred/audio/song.flac'},muted:false}));
 expect(onChange).toHaveBeenCalledWith({sourceDuration:27});onChange.mockClear();
 fireEvent.click(screen.getByRole('button',{name:key('play')}));
 expect(invoke).toHaveBeenCalledWith('native_video_update',expect.objectContaining({action:{kind:'crop',start:2,end:5}}));
 await update({paused:false,position:3.25});
 fireEvent.click(screen.getByRole('button',{name:key('setIn')}));expect(onChange).toHaveBeenLastCalledWith({trimStart:3.25});
 fireEvent.click(screen.getByRole('button',{name:key('setOut')}));expect(onChange).toHaveBeenLastCalledWith({trimEnd:3.25});
 fireEvent.click(screen.getByRole('button',{name:key('pause')}));
 expect(invoke).toHaveBeenCalledWith('native_video_update',expect.objectContaining({action:{kind:'pause',paused:true}}));
 await update({paused:true});fireEvent.click(screen.getByRole('button',{name:key('play')}));
 expect(vi.mocked(invoke).mock.calls.filter(([,a])=>(a as {action?:{kind:string}})?.action?.kind==='crop')).toHaveLength(2);
});

test('native FLAC blank bounds and invalid status never rewrite saved trims',async()=>{
 const {update}=linuxAudio();const r={...flac(),trimStart:undefined,trimEnd:undefined};
 const {onChange,rerender}=mount(r);await act(async()=>{});
 for(const name of ['studio.run.video.trimStartOf','studio.run.video.trimEndOf'])expect((screen.getByRole('spinbutton',{name}) as HTMLInputElement).value).toBe('');
 fireEvent.click(screen.getByRole('button',{name:key('play')}));
 expect(invoke).toHaveBeenCalledWith('native_video_update',expect.objectContaining({action:{kind:'crop',start:0,end:27}}));
 for(const duration of [0,NaN,Infinity]){await update({duration});expect((screen.getByRole('button',{name:key('play')}) as HTMLButtonElement).disabled).toBe(true);}
 expect(onChange.mock.calls.every(([x])=>Object.keys(x).join()==='sourceDuration'&&x.sourceDuration===27)).toBe(true);
 await update({duration:27});rerender(<ReferenceMedia reference={{...r,trimStart:30,trimEnd:35}} onChange={onChange}/>);
 expect((screen.getByRole('button',{name:key('play')}) as HTMLButtonElement).disabled).toBe(true);
 expect((screen.getByRole('spinbutton',{name:'studio.run.video.trimEndOf'}) as HTMLInputElement).value).toBe('35');
});

test('native errors show the concrete cause, log once and preserve saved metadata and trims',async()=>{
 const {update}=linuxAudio();const {onChange}=mount({...flac(),sourceDuration:27});await act(async()=>{});onChange.mockClear();
 fireEvent.click(screen.getByRole('button',{name:key('play')}));
 await update({paused:false,error:'libmpv: cannot decode audio'});await update({});
 expect(screen.getAllByRole('alert').some(el=>el.textContent===`${key('loadError')} libmpv: cannot decode audio`)).toBe(true);
 expect(vi.mocked(invoke).mock.calls.filter(([cmd])=>cmd==='native_logs_reference_media_error')).toHaveLength(1);
 expect(invoke).toHaveBeenCalledWith('native_video_update',expect.objectContaining({action:{kind:'pause',paused:true}}));
 expect((screen.getByRole('button',{name:key('play')}) as HTMLButtonElement).disabled).toBe(true);
 expect(onChange).not.toHaveBeenCalled();
 expect((screen.getByRole('spinbutton',{name:'studio.run.video.trimEndOf'}) as HTMLInputElement).value).toBe('5');
});

test('native crop stops on trim edits and source replacement/unmount close only their owner',async()=>{
 linuxAudio();const r=flac();const {rerender,onChange,unmount}=mount(r);await act(async()=>{});
 const owner=()=>(vi.mocked(invoke).mock.calls.filter(([cmd])=>cmd==='native_video_open').at(-1)![1] as {id:string}).id;
 const old=owner();fireEvent.click(screen.getByRole('button',{name:key('play')}));
 rerender(<ReferenceMedia reference={{...r,trimEnd:8}} onChange={onChange}/>);
 expect(invoke).toHaveBeenCalledWith('native_video_update',{id:old,action:{kind:'pause',paused:true}});
 const next={...r,item:{...r.item,path:'/input/song.flac'}};
 rerender(<ReferenceMedia reference={next} onChange={onChange}/>);await act(async()=>{});
 expect(invoke).toHaveBeenCalledWith('native_video_update',{id:old,action:{kind:'close'}});
 expect(owner()).not.toBe(old);const current=owner();unmount();
 expect(invoke).toHaveBeenCalledWith('native_video_update',{id:current,action:{kind:'close'}});expect(vi.getTimerCount()).toBe(0);
});

test('native audio transport stays visible only inside its containing modal',async()=>{
 linuxAudio();vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockReturnValue({left:10,top:10,right:210,bottom:50,width:200,height:40,x:10,y:10,toJSON:()=>({})});
 Object.defineProperty(document,'elementFromPoint',{configurable:true,value:()=>document.querySelector('.native-video-controls')});
 const ui=render(<div role="dialog" aria-modal="true"><ReferenceMedia reference={flac()} onChange={vi.fn()}/></div>);await act(async()=>{});
 const bounds=(visible:boolean)=>expect(invoke).toHaveBeenCalledWith('native_video_update',expect.objectContaining({action:expect.objectContaining({kind:'bounds',visible,audioOnly:true})}));
 bounds(true);vi.mocked(invoke).mockClear();const covering=document.createElement('dialog');covering.open=true;
 try{await act(async()=>{document.body.append(covering);});bounds(false);vi.mocked(invoke).mockClear();await act(async()=>covering.remove());bounds(true);}finally{covering.remove();ui.unmount();}
});

test.each(['browser','Windows','pathless Linux'])('FLAC keeps the media-element fallback in %s',async(mode)=>{
 vi.mocked(invoke).mockClear();vi.mocked(isTauri).mockReturnValue(mode!=='browser');
 vi.spyOn(navigator,'userAgent','get').mockReturnValue(mode==='Windows'?'Windows':'Linux');
 const r=flac();if(mode==='pathless Linux')delete r.item.path;
 const {container,metadata,onChange}=mount(r);metadata(27);await act(async()=>{});
 expect(container.querySelector('audio')).toBeTruthy();expect(container.querySelector('[data-native-video]')).toBeNull();
 expect(onChange).toHaveBeenCalledWith({sourceDuration:27});expect(invoke).not.toHaveBeenCalled();
});

test('Linux video references reuse the native player and crop control with source metadata',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);
 vi.spyOn(navigator,'userAgent','get').mockReturnValue('Linux DEAS');
 vi.mocked(invoke).mockClear().mockImplementation(async(command,args)=>command==='native_video_update'&&(args as {action:{kind:string}}).action.kind==='status'?{position:2,duration:8,width:320,height:180,paused:true,muted:true,volume:100}:undefined);
 const {container,onChange}=mount(reference({item:{name:'clip.mp4',path:'/clip.mp4',src:'asset://clip.mp4',media:'video'}}));
 await act(async()=>{});
 expect(container.querySelector('video,audio')).toBeNull();
 expect(container.querySelector('[data-native-video]')).toBeTruthy();
 expect(invoke).toHaveBeenCalledWith('native_video_open',expect.objectContaining({source:{path:'/clip.mp4'},muted:true}));
 expect(onChange).toHaveBeenCalledWith({sourceDuration:8});
 fireEvent.click(screen.getByRole('button',{name:key('play')}));
 expect(invoke).toHaveBeenCalledWith('native_video_update',expect.objectContaining({action:{kind:'crop',start:2,end:5}}));
 fireEvent.click(screen.getByRole('button',{name:key('setOut')}));
 expect(onChange).toHaveBeenLastCalledWith({trimEnd:2});
});

