import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import { MEDIA_DRAG, type MediaItem } from '../../lib/media';
import { ReferencesPanel, directorTags, firstVisualRef, forgeLabels, fromPack, h3ForgeContext, prefillItems, refsProblem, slotsOf, toPack, type VideoRef } from './ReferencesPanel';

const os=vi.hoisted(()=>({drop:null as null|((e:{payload:unknown})=>void)}));
vi.mock('@tauri-apps/api/core',()=>({isTauri:vi.fn(()=>false),invoke:vi.fn(),convertFileSrc:(p:string)=>`asset://${p}`}));
vi.mock('@tauri-apps/api/webview',()=>({getCurrentWebview:()=>({onDragDropEvent:async(h:(e:{payload:unknown})=>void)=>{os.drop=h;return()=>{};}})}));
const dialogMethods=Object.getOwnPropertyDescriptors(HTMLDialogElement.prototype);
beforeEach(()=>{localStorage.setItem('deas.locale.v1','en');Object.defineProperties(HTMLDialogElement.prototype,{showModal:{configurable:true,value:function(this:HTMLDialogElement){this.open=true;}},close:{configurable:true,value:function(this:HTMLDialogElement){this.open=false;}}});});
afterEach(()=>{cleanup();vi.restoreAllMocks();for(const name of ['showModal','close'])if(dialogMethods[name])Object.defineProperty(HTMLDialogElement.prototype,name,dialogMethods[name]);else Reflect.deleteProperty(HTMLDialogElement.prototype,name);vi.mocked(isTauri).mockReturnValue(false);os.drop=null;});
const item=(name:string,media:MediaItem['media']):MediaItem=>({name,media,src:`asset://${name}`,path:`/in/${name}`});
const ref=(name:string,media:MediaItem['media'],x:Partial<VideoRef>={}):VideoRef=>({item:item(name,media),second:0,strength:1,mediaMode:'video',...x});

test.each([
 ['C:\\env\\in\\','C:\\env\\in\\sub\\a.png'],
 ['C:/env/in/','C:\\env\\in\\sub\\a.png'],
 ['/env/in/','/env/in/sub/a.png'],
])('reference packs and Forge share protocol-relative names (%s)',(inputRoot,path)=>{
 const refs=[ref('a.png','image',{item:{...item('a.png','image'),path}})];
 const {pack,skipped}=toPack(refs,inputRoot);
 expect(pack.items[0].value).toBe('sub/a.png');expect(skipped).toEqual([]);
 const context=h3ForgeContext({pipeline:'i2v',media:refs[0].item,lastFrame:null,refs:[],sourceAudio:false,seconds:5,canvas:{width:512,height:512},inputRoot});
 expect(context.references[0].path).toBe('sub/a.png');
 expect(fromPack(pack,inputRoot)!.adds[0].path).toBe(inputRoot.replace(/\\/g,'/').replace(/\/+$/,'')+'/sub/a.png');
});
test.each([
 ['/env/in','/env/out','/env/out/sub/a.png'],
 ['C:\\env\\in','C:\\env\\out','C:\\env\\out\\sub\\a.png'],
])('Forge preserves output reference and first/last frame paths (%s)',(inputRoot,outputRoot,path)=>{
 const image={...item('a.png','image'),path};
 const base={media:image,lastFrame:image,refs:[ref('a.png','image',{item:image})],sourceAudio:false,seconds:5,canvas:{width:512,height:512},inputRoot,outputRoot};
 expect(h3ForgeContext({...base,pipeline:'i2v'}).references.map(r=>r.path)).toEqual(['sub/a.png [output]','sub/a.png [output]']);
 expect(h3ForgeContext({...base,pipeline:'ref2v'}).references[0].path).toBe('sub/a.png [output]');
 expect(h3ForgeContext({...base,pipeline:'ref2v',refs:[ref('other.png','image',{item:{...image,path:'/outside/a.png'}})]}).references[0].path).toBeUndefined();
});
test('reference packs never rewrite Linux backslash filenames into different files',()=>{
 const refs=[ref('a\\b.png','image')];
 expect(toPack(refs,'/in').skipped).toEqual(['a\\b.png']);
 const context=h3ForgeContext({pipeline:'i2v',media:refs[0].item,lastFrame:null,refs:[],sourceAudio:false,seconds:5,canvas:{width:512,height:512},inputRoot:'/in'});
 expect(context.references[0].path).toBeUndefined();
});

test('Director labels follow the node load order: images, then video items (picture and/or sound), then audio files, each by slot',()=>{
 // An audio file picked before a sound-only video still comes after it, as in the Director's prefill and the node.
 const refs=[ref('song.wav','audio'),ref('talk.mp4','video',{mediaMode:'audio'}),ref('walk.mp4','video',{mediaMode:'video_audio'}),ref('a.png','image'),ref('b.png','image')];
 expect(slotsOf(refs)).toEqual([0,1,0,0,1]);
 expect(directorTags(refs)).toEqual(['<Audio 3>','<Audio 2>','<Video 1> <Audio 1>','<Picture 1>','<Picture 2>']);
});

test('limits and trims: too many sound tracks or a trim outside 2–15 s is a run problem',()=>{
 expect(refsProblem([ref('a.png','image')])).toBe('');
 expect(refsProblem([1,2,3,4].map(n=>ref(`${n}.wav`,'audio')))).toBe('studio.run.video.refFull');
 expect(refsProblem([ref('s.wav','audio',{trimStart:3,trimEnd:4})])).toBe('studio.run.video.trimInvalid');
 expect(refsProblem([ref('s.wav','audio',{trimStart:3,trimEnd:18.5})])).toBe('studio.run.video.trimInvalid');
 expect(refsProblem([ref('s.wav','audio',{trimStart:3})])).toBe(''); // the clip length is the node's to check
});

test('HyperFlow retains unsupported saved references for explicit removal',()=>{
 const onChange=vi.fn();const refs=[ref('a.png','image'),ref('clip.mp4','video'),ref('voice.wav','audio')];
 render(<I18nProvider><ReferencesPanel refs={refs} onChange={onChange} family="minimax-h3-hyperflow" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()}/></I18nProvider>);
 expect(screen.getByRole('region',{name:'Videos'})).toBeTruthy();
 expect(screen.getByRole('region',{name:'Audio'})).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Remove reference clip.mp4'}));
 expect(onChange).toHaveBeenLastCalledWith([refs[0],refs[2]]);
});

test('the panel reorders inside a lane, edits trims, inserts the label and removes',()=>{
 const onChange=vi.fn(),onInsert=vi.fn();
 const refs=[ref('a.png','image'),ref('s.wav','audio'),ref('b.png','image')];
 render(<I18nProvider><ReferencesPanel refs={refs} onChange={onChange} family="minimax-h3" seconds={5} label={(_,i)=>directorTags(refs)[i]} onAdd={vi.fn()} onInsert={onInsert}/></I18nProvider>);
 expect(screen.getByText('Images 2/9 · Videos 0/3 · Sound 1/3 · Files 3/12')).toBeTruthy();
 expect((screen.getByRole('button',{name:'Move a.png earlier'}) as HTMLButtonElement).disabled).toBe(true);
 expect((screen.getByRole('button',{name:'Move s.wav later'}) as HTMLButtonElement).disabled).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'Move a.png later'}));
 expect(onChange.mock.lastCall![0].map((r:VideoRef)=>r.item.name)).toEqual(['b.png','s.wav','a.png']); // swaps with the next image, skipping the audio
 fireEvent.click(screen.getByRole('button',{name:'Edit s.wav'}));
 fireEvent.change(screen.getByLabelText('End of s.wav'),{target:{value:'7.5'}});
 expect(onChange.mock.lastCall![0][1]).toMatchObject({trimEnd:7.5});
 expect(screen.queryByLabelText('Start of a.png')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'Close'}));
 fireEvent.click(screen.getByRole('button',{name:'Insert <Picture 2> into the prompt'}));
 expect(onInsert).toHaveBeenCalledWith('<Picture 2>');
 fireEvent.click(screen.getByRole('button',{name:'Remove reference s.wav'}));
 expect(onChange.mock.lastCall![0].map((r:VideoRef)=>r.item.name)).toEqual(['a.png','b.png']);
});

const mount=(refs:VideoRef[])=>{const onChange=vi.fn(),onAdd=vi.fn();render(<I18nProvider><ReferencesPanel refs={refs} onChange={onChange} family="minimax-h3" seconds={5} label={()=>''} onAdd={onAdd} onInsert={vi.fn()} inputRoot="/env/in"/></I18nProvider>);return {onChange,onAdd};};
const card=(name:string)=>document.querySelector(`[data-ref="${name}"]`)!;

test('dragging a card onto another of its lane moves it there; other lanes ignore it; a dock tile dropped on the tab is added',()=>{
 const {onChange,onAdd}=mount([ref('a.png','image'),ref('s.wav','audio'),ref('b.png','image')]);
 const none={types:[] as string[],getData:()=>''};
 fireEvent.dragStart(card('a.png').querySelector('strong')!,{dataTransfer:{setData:vi.fn(),effectAllowed:''}});
 fireEvent.drop(card('s.wav'),{dataTransfer:none});
 expect(onChange).not.toHaveBeenCalled();
 fireEvent.dragStart(card('a.png').querySelector('strong')!,{dataTransfer:{setData:vi.fn(),effectAllowed:''}});
 fireEvent.drop(card('b.png'),{dataTransfer:none});
 expect(onChange.mock.lastCall![0].map((r:VideoRef)=>r.item.name)).toEqual(['s.wav','b.png','a.png']);
 const tile=item('clip.mp4','video');
 fireEvent.drop(document.querySelector('[data-references]')!,{dataTransfer:{types:[MEDIA_DRAG],getData:(k:string)=>k===MEDIA_DRAG?JSON.stringify(tile):''}});
 expect(onAdd).toHaveBeenCalledWith([{item:tile}]);
});

test('native OS file drops add references only when dropped over the tab',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);
 vi.mocked(invoke).mockResolvedValue([{name:'v.wav',path:'/os/v.wav',kind:'file',symlink:false,media:'audio',size:1,modified:1}]);
 const {onAdd}=mount([]);
 await waitFor(()=>expect(os.drop).not.toBeNull());
 Object.defineProperty(document.querySelector('[data-references]')!,'getBoundingClientRect',{value:()=>({left:0,top:0,right:100,bottom:100})});
 os.drop!({payload:{type:'drop',paths:['/os/v.wav'],position:{x:500*devicePixelRatio,y:10}}});
 expect(invoke).not.toHaveBeenCalled();
 os.drop!({payload:{type:'drop',paths:['/os/v.wav'],position:{x:20*devicePixelRatio,y:20*devicePixelRatio}}});
 await waitFor(()=>expect(onAdd).toHaveBeenCalledWith([{item:{name:'v.wav',path:'/os/v.wav',media:'audio',src:'asset:///os/v.wav'}}]));
 expect(invoke).toHaveBeenCalledWith('native_media_describe',{paths:['/os/v.wav']});
});

test('a picture pasted inside the tab is saved into Input and added; text-field and outside pastes are left alone',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_media_paste'?'/env/in/pasted-20261108-153000.png'
 :command==='native_media_describe'?(args as {paths:string[]}).paths.map(path=>({name:path.split('/').at(-1),path,kind:'file',symlink:false,media:'image',size:1,modified:1})):null);
 const {onAdd}=mount([]);
 await waitFor(()=>expect(os.drop).not.toBeNull());
 const picture=Object.assign(new File(['x'],'shot.png',{type:'image/png'}),{arrayBuffer:async()=>new Uint8Array([1,2]).buffer}); // jsdom Files lack arrayBuffer
 const paste=(target:Element)=>{const e=new Event('paste',{bubbles:true,cancelable:true});Object.defineProperty(e,'clipboardData',{value:{files:[picture]}});target.dispatchEvent(e);return e;};
 expect(paste(document.body).defaultPrevented).toBe(false);
 const panel=document.querySelector('[data-references]')!;
 const field=document.createElement('textarea');panel.append(field);
 expect(paste(field).defaultPrevented).toBe(false);field.remove();
 expect(invoke).not.toHaveBeenCalledWith('native_media_paste',expect.anything());
 expect(paste(panel).defaultPrevented).toBe(true);
 await waitFor(()=>expect(onAdd).toHaveBeenCalledWith([{item:expect.objectContaining({name:'pasted-20261108-153000.png',path:'/env/in/pasted-20261108-153000.png',media:'image'})}]));
 expect(invoke).toHaveBeenCalledWith('native_media_paste',{bytes:[1,2],stamp:expect.stringMatching(/^\d{8}-\d{6}$/)});
});

const at=(name:string,media:MediaItem['media'],dir='/env/in')=>({name,media,src:`asset://${dir}/${name}`,path:`${dir}/${name}`});
const refAt=(name:string,media:MediaItem['media'],x:Partial<VideoRef>={},dir='/env/in'):VideoRef=>({item:at(name,media,dir),second:0,strength:1,mediaMode:'video',...x});

test('Director reference packs: saved in the node\'s files format (type, then lane slot as _rank), input-relative, outside files skipped; read back safely',()=>{
 const refs=[refAt('voice.wav','audio',{trimStart:1,trimEnd:4}),refAt('deas-input-v.mp4','video',{mediaMode:'audio'}),refAt('sub/a.png','image'),refAt('out.png','image',{},'/env/out')];
 const {pack,skipped}=toPack(refs,'/env/in/',[{name:'chars/hero',strength:0.5,description:'the hero'}]);
 expect(skipped).toEqual(['out.png']);
 expect(pack).toMatchObject({dasiwa_minimax_h3_reference_pack:true,schema_version:1,model_mode:'REF2VA',items:[
  {type:'image',value:'sub/a.png',_rank:0},{type:'video',value:'deas-input-v.mp4',media_mode:'audio',_rank:0},{type:'audio',value:'voice.wav',trim_start:1,trim_end:4,_rank:0}],refmods:[{slot:1,name:'chars/hero',enabled:true,strength:0.5,description:'the hero'}]});
 expect(fromPack(JSON.parse(JSON.stringify(pack)),'/env/in')).toEqual({invalid:0,refmods:[{name:'chars/hero',strength:0.5,description:'the hero'}],adds:[{path:'/env/in/sub/a.png'},{path:'/env/in/deas-input-v.mp4',mediaMode:'audio'},{path:'/env/in/voice.wav',trimStart:1,trimEnd:4}]});
 // As the Director writes it: trim_start 0 / trim_end '' mean untrimmed; unsafe names never leave the input folder.
 const director={dasiwa_minimax_h3_reference_pack:true,schema_version:1,model_mode:'REF2VA',items:[
  {type:'audio',value:'s.wav',_rank:0,trim_start:0,trim_end:''},{type:'image',value:'b.png',_rank:1},{type:'image',value:'a.png',_rank:0},
  {type:'image',value:'../etc/x.png'},{type:'image',value:'/abs.png'},{type:'image',value:'C:/x.png'},{type:'image',value:'a\\b.png'},{type:'model',value:'m.bin'}],
  // Director RefMod rows: slot order, default strength 1, numeric strings, disabled rows dropped, bad names counted.
  refmods:[{slot:4,name:'place',strength:'0.5'},{slot:2,name:'hero'},{slot:3,name:'off',enabled:false},{slot:5,name:'../x',strength:1}]};
 expect(fromPack(director,'C:\\env\\in')).toEqual({invalid:6,refmods:[{name:'hero',strength:1,description:''},{name:'place',strength:0.5,description:''}],adds:[{path:'C:/env/in/a.png'},{path:'C:/env/in/b.png'},{path:'C:/env/in/s.wav'}]});
 expect(fromPack({items:[]},'/env/in')).toBeNull();
});

test('pack Save/Load go through the native JSON dialogs; missing files are reported, not added; Clear empties the list',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);
 const pack={dasiwa_minimax_h3_reference_pack:true,schema_version:1,model_mode:'REF2VA',items:[{type:'image',value:'a.png',_rank:0},{type:'video',value:'gone.mp4',_rank:0,media_mode:'video_audio'}]};
 vi.mocked(invoke).mockImplementation(async(c,args)=>{
  if(c==='native_json_open')return JSON.stringify(pack);
  if(c==='native_json_save')return true;
  const [p]=(args as {paths:string[]}).paths;if(p.endsWith('gone.mp4'))throw Error('missing');
  return [{name:'a.png',path:p,kind:'file',symlink:false,media:'image',size:1,modified:1}];
 });
 const {onChange,onAdd}=mount([refAt('a.png','image'),refAt('far.png','image',{},'/elsewhere')]);
 fireEvent.click(screen.getByRole('button',{name:'Load pack (replace)…'}));
 await waitFor(()=>expect(onAdd).toHaveBeenCalledWith([{item:{name:'a.png',path:'/env/in/a.png',media:'image',src:'asset:///env/in/a.png'}}],true));
 expect(screen.getByRole('status').textContent).toBe('Loaded 1 references · 1 missing or invalid skipped');
 fireEvent.click(screen.getByRole('button',{name:'Save pack…'}));
 await waitFor(()=>expect(screen.getByRole('status').textContent).toBe('Saved 1 references · skipped (not in the input folder): far.png'));
 const saved=JSON.parse((vi.mocked(invoke).mock.calls.find(([c])=>c==='native_json_save')![1] as {text:string}).text);
 expect(saved.items).toEqual([{type:'image',value:'a.png',_rank:0}]);
 fireEvent.click(screen.getByRole('button',{name:'Clear'}));
 expect(onChange).toHaveBeenCalledWith([]);
});

test('RefMods: read the library on demand, add, edit strength/description, insert the alias, remove',async()=>{
 const onRefmods=vi.fn(),onInsert=vi.fn();
 const library=vi.fn(async()=>[{name:'chars/hero',kinds:['image' as const,'audio' as const],description:'the hero'}]);
 const {rerender}=render(<I18nProvider><ReferencesPanel refs={[]} onChange={vi.fn()} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={onInsert} refmods={[]} onRefmods={onRefmods} library={library}/></I18nProvider>);
 expect(library).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'Read RefMod library'}));
 expect((await screen.findByRole('option',{name:'chars/hero (image + audio)'}))).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Add RefMod'}));
 expect(onRefmods).toHaveBeenLastCalledWith([{name:'chars/hero',strength:1,description:'the hero',kinds:['image','audio']}]);
 const rows=[{name:'chars/hero',strength:1,description:'the hero'}];
 rerender(<I18nProvider><ReferencesPanel refs={[]} onChange={vi.fn()} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={onInsert} refmods={rows} onRefmods={onRefmods} library={library}/></I18nProvider>);
 fireEvent.change(screen.getByLabelText('Strength of chars/hero'),{target:{value:'0.4'}});
 expect(onRefmods).toHaveBeenLastCalledWith([{name:'chars/hero',strength:0.4,description:'the hero'}]);
 fireEvent.change(screen.getByLabelText('Description of chars/hero'),{target:{value:'red coat'}});
 expect(onRefmods).toHaveBeenLastCalledWith([{name:'chars/hero',strength:1,description:'red coat'}]);
 fireEvent.click(screen.getByRole('button',{name:'Insert <RefMod 1> into the prompt'}));
 expect(onInsert).toHaveBeenCalledWith('<RefMod 1>');
 fireEvent.click(screen.getByRole('button',{name:'Remove reference chars/hero'}));
 expect(onRefmods).toHaveBeenLastCalledWith([]);
});

test.each([false,true])('a pack loaded after the library hydrates RefMod kinds from cached entries (replace=%s)',async replace=>{
 vi.mocked(isTauri).mockReturnValue(true);
 const library=vi.fn(async()=>[{name:'chars/hero',kinds:['image' as const,'audio' as const],description:'library description'}]);
 const rows=[{name:'chars/hero',strength:0.4,description:'pack description'},{name:'unknown',strength:1,description:''}];
 vi.mocked(invoke).mockImplementation(async c=>c==='native_json_open'?JSON.stringify(toPack([],'/env/in',rows).pack):undefined);
 const onRefmods=vi.fn();
 render(<I18nProvider><ReferencesPanel refs={[]} onChange={vi.fn()} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()} inputRoot="/env/in" refmods={[{name:'existing',strength:0.7,description:'keep',kinds:['video']}]} onRefmods={onRefmods} library={library}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Read RefMod library'}));
 await screen.findByRole('option',{name:'chars/hero (image + audio)'});
 onRefmods.mockClear();
 fireEvent.click(screen.getByRole('button',{name:replace?'Load pack (replace)…':'Load pack (add)…'}));
 const incoming=[{...rows[0],kinds:['image','audio']},{...rows[1],kinds:undefined}];
 await waitFor(()=>expect(onRefmods).toHaveBeenLastCalledWith([...(replace?[]:[{name:'existing',strength:0.7,description:'keep',kinds:['video']}]),...incoming]));
 expect(library).toHaveBeenCalledTimes(1);
});

test('Save pack is disabled when empty and saves RefMods-only packs through the native dialog',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);
 vi.mocked(invoke).mockResolvedValue(true);
 const rows=[{name:'chars/hero',strength:0.4,description:'red coat',kinds:['image']}];
 const view=(refmods:typeof rows)=><I18nProvider><ReferencesPanel refs={[]} onChange={vi.fn()} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()} inputRoot="/env/in" refmods={refmods} onRefmods={vi.fn()}/></I18nProvider>;
 const {rerender}=render(view([]));
 expect((screen.getByRole('button',{name:'Save pack…'}) as HTMLButtonElement).disabled).toBe(true);
 rerender(view(rows));
 expect((screen.getByRole('button',{name:'Save pack…'}) as HTMLButtonElement).disabled).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'Save pack…'}));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_json_save',expect.anything()));
 const saved=JSON.parse((vi.mocked(invoke).mock.calls.find(([c])=>c==='native_json_save')![1] as {text:string}).text);
 expect(saved.items).toEqual([]);
 expect(saved.refmods).toEqual([{slot:1,name:'chars/hero',enabled:true,strength:0.4,description:'red coat'}]);
});

test('Director Forge context follows the intent: frame modes from first/last frame, REF2VA references in node order with input paths only',()=>{
 const canvas={width:1024,height:576},base={media:null,lastFrame:null,refs:[],sourceAudio:false,seconds:5,canvas,inputRoot:'/env/in'};
 expect(h3ForgeContext({...base,pipeline:'t2v'})).toEqual({mode:'T2VA',duration:5,canvas,references:[],easy:false});
 const first=at('a.png','image'),last=at('z.png','image','/elsewhere');
 expect(h3ForgeContext({...base,pipeline:'t2v',lastFrame:last}).mode).toBe('L2VA');
 expect(h3ForgeContext({...base,pipeline:'i2v',media:first,lastFrame:last})).toEqual({mode:'FL2VA',duration:5,canvas,easy:false,references:[
  {kind:'image',path:'a.png',role:'keyframe',subject_group:''},{kind:'image',role:'keyframe',subject_group:''}]});
 expect(h3ForgeContext({...base,pipeline:'v2v',media:at('v.mp4','video'),sourceAudio:true}).references).toEqual([{kind:'video',role:'motion',stream:'both'}]);
 const refs=[refAt('s.wav','audio',{trimStart:1,trimEnd:4}),refAt('v.mp4','video',{mediaMode:'audio'}),refAt('p.png','image')];
 expect(h3ForgeContext({...base,pipeline:'ref2v',refs}).references).toEqual([
  {kind:'image',path:'p.png',role:'subject',subject_group:'A',easy_role:'character-1'},{kind:'video',role:'motion',stream:'audio'},{kind:'audio',duration_seconds:3}]);
 expect(h3ForgeContext({...base,pipeline:'ref2v',refs}).easy).toBe(false); // labels apply only when every reference is a picture
});

test('frame Forge context preserves explicit people, axes and boxes without inventing identities',()=>{
 const first=at('a.png','image'),last=at('z.png','image');
 const boxes=[{n:4,box:[0.1,0.2,0.6,0.8] as [number,number,number,number]}];
 const base={pipeline:'i2v',media:first,lastFrame:last,refs:[],sourceAudio:false,seconds:5,canvas:{width:64,height:64},inputRoot:'/env/in',framePeople:{[first.src]:{who:[2,1],whoAxis:'y' as const},[last.src]:{who:[3],boxes}}};
 const context=h3ForgeContext(base);
 expect(context.mode).toBe('FL2VA');expect(context.easy).toBe(false);
 expect(context.references).toEqual([
  {kind:'image',path:'a.png',role:'keyframe',subject_group:'',who:[2,1],who_axis:'y'},
  {kind:'image',path:'z.png',role:'keyframe',subject_group:'',who:[3],boxes}]);
 expect(h3ForgeContext({...base,lastFrame:null}).references).toEqual([context.references[0]]);
 expect(h3ForgeContext({...base,pipeline:'t2v'}).references).toEqual([context.references[1]]);
 expect(h3ForgeContext({...base,media:at('new.png','image'),lastFrame:null}).references[0]).not.toHaveProperty('who');
});

test('Director picture labels: chosen labels stay, the rest take the next free Character; roles, groups and instructions reach the Forge; packs keep them',()=>{
 const canvas={width:1024,height:576};
 const refs=[refAt('a.png','image'),refAt('b.png','image',{label:'character-1'}),refAt('c.png','image',{label:'place',instructions:' the old harbour '}),refAt('d.png','image',{label:'custom',instructions:'the coat'})];
 expect(forgeLabels(refs)).toEqual(['character-2','character-1','place','custom']);
 expect(forgeLabels([refAt('s.wav','audio'),refAt('x.png','image',{label:'bogus'})])).toEqual(['','character-1']);
 const ctx=h3ForgeContext({pipeline:'ref2v',media:null,lastFrame:null,refs,sourceAudio:false,seconds:5,canvas,inputRoot:'/env/in'});
 expect(ctx.easy).toBe(true);
 expect(ctx.references).toEqual([
  {kind:'image',path:'a.png',role:'subject',subject_group:'B',easy_role:'character-2'},
  {kind:'image',path:'b.png',role:'subject',subject_group:'A',easy_role:'character-1'},
  {kind:'image',path:'c.png',role:'subject',subject_group:'',easy_role:'place',instructions:'the old harbour'},
  {kind:'image',path:'d.png',role:'custom',subject_group:'',easy_role:'custom',instructions:'the coat'}]);
 const {pack}=toPack(refs,'/env/in');
 expect(pack.items.map(i=>[(i as {forge_label?:string}).forge_label,(i as {forge_instructions?:string}).forge_instructions])).toEqual([[undefined,undefined],['character-1',undefined],['place','the old harbour'],['custom','the coat']]);
 expect(fromPack(pack,'/env/in')!.adds.map(a=>[a.label,a.instructions])).toEqual([[undefined,undefined],['character-1',undefined],['place','the old harbour'],['custom','the coat']]);
});

test('Director groups reserve their character numbers, keep left-to-right order in Forge and packs, and ignore unknown groups',()=>{
 const refs=[refAt('pair.png','image',{label:'group-21'}),refAt('single.png','image'),refAt('other.png','image',{label:'group-13'})];
 expect(forgeLabels(refs)).toEqual(['group-21','character-4','group-13']);
 const ctx=h3ForgeContext({pipeline:'ref2v',media:null,lastFrame:null,refs,sourceAudio:false,seconds:5,canvas:{width:64,height:64},inputRoot:'/env/in'});
 expect(ctx.easy).toBe(true);
 expect(ctx.references[0]).toEqual({kind:'image',path:'pair.png',role:'subject',subject_group:'',easy_role:'group-21'});
 const {pack}=toPack(refs,'/env/in');
 expect(fromPack(pack,'/env/in')!.adds.map(a=>a.label)).toEqual(['group-21',undefined,'group-13']);
 expect(forgeLabels([refAt('bad.png','image',{label:'group-99'}),refAt('auto.png','image')])).toEqual(['character-1','character-2']);
});

test('nine references with a group can assign characters above nine using the Director subject groups',()=>{
 const refs=[refAt('group.png','image',{label:'group-123'}),...Array.from({length:8},(_,i)=>refAt(`${i}.png`,'image'))];
 const ctx=h3ForgeContext({pipeline:'ref2v',media:null,lastFrame:null,refs,sourceAudio:false,seconds:5,canvas:{width:64,height:64},inputRoot:'/env/in'});
 expect(ctx.references.at(-1)).toMatchObject({easy_role:'character-11',subject_group:'K'});
 const {pack}=toPack([refAt('last.png','image',{label:'character-32'})],'/env/in');
 expect(fromPack(pack,'/env/in')!.adds[0].label).toBe('character-32');
});

test.each(['y','z','none'] as const)('explicit character order %s reaches Forge and Director packs while reserving the actual members',axis=>{
 const refs=[refAt('pair.png','image',{label:'character-2',who:[2,4],whoAxis:axis}),refAt('auto.png','image')];
 const ctx=h3ForgeContext({pipeline:'ref2v',media:null,lastFrame:null,refs,sourceAudio:false,seconds:5,canvas:{width:64,height:64},inputRoot:'/env/in'});
 expect(ctx.references[0]).toMatchObject({easy_role:'character-2',picture_kinds:['character'],who:[2,4],who_axis:axis});
 expect(forgeLabels([...refs,refAt('auto2.png','image')])).toEqual(['character-2','character-1','character-3']);
 const {pack}=toPack(refs,'/env/in');
 expect(pack.items[0]).toMatchObject({forge_kinds:'character',forge_who:'2,4',forge_who_axis:axis});
 expect(fromPack(pack,'/env/in')!.adds[0]).toMatchObject({label:'character-2',who:[2,4],whoAxis:axis});
});

test('the character buttons keep tap order, retain one character, and a new label clears overrides',()=>{
 const onChange=vi.fn();let refs=[refAt('a.png','image',{label:'character-2'})];
 const view=()=> <I18nProvider><ReferencesPanel refs={refs} onChange={onChange} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()}/></I18nProvider>;
 const {rerender}=render(view());
 fireEvent.click(screen.getByRole('button',{name:'Edit a.png'}));fireEvent.click(screen.getByText('People & order'));
 fireEvent.click(screen.getByRole('button',{name:'Character 4 in a.png'}));
 expect(onChange.mock.lastCall![0][0]).toMatchObject({who:[2,4]});
 refs=onChange.mock.lastCall![0];rerender(view());
 fireEvent.change(screen.getByLabelText('Character order in a.png'),{target:{value:'z'}});
 expect(onChange.mock.lastCall![0][0]).toMatchObject({who:[2,4],whoAxis:'z'});
 refs=onChange.mock.lastCall![0];rerender(view());
 fireEvent.click(screen.getByRole('button',{name:'Character 2 in a.png'}));
 expect(onChange.mock.lastCall![0][0]).toMatchObject({who:[4],label:'character-4'});
 refs=onChange.mock.lastCall![0];rerender(view());
 expect(screen.getByRole('button',{name:'Character 4 in a.png'})).toHaveProperty('disabled',true);
 fireEvent.change(screen.getByLabelText('Forge label of a.png'),{target:{value:'place'}});
 expect(onChange.mock.lastCall![0][0]).toMatchObject({label:'place',who:undefined,whoAxis:undefined});
});

test('spatial order updates the legacy label without claiming left-to-right for a vertical group',()=>{
 const onChange=vi.fn();let refs=[refAt('pair.png','image',{label:'group-21'})];
 const view=()=> <I18nProvider><ReferencesPanel refs={refs} onChange={onChange} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()}/></I18nProvider>;
 const {rerender}=render(view());fireEvent.click(screen.getByRole('button',{name:'Edit pair.png'}));fireEvent.click(screen.getByText('People & order'));
 fireEvent.change(screen.getByLabelText('Character order in pair.png'),{target:{value:'y'}});
 expect(onChange.mock.lastCall![0][0]).toMatchObject({label:'character-2',who:[2,1],whoAxis:'y'});
 refs=onChange.mock.lastCall![0];rerender(view());
 fireEvent.change(screen.getByLabelText('Character order in pair.png'),{target:{value:'x'}});
 expect(onChange.mock.lastCall![0][0]).toMatchObject({label:'group-21',who:[2,1],whoAxis:undefined});
});

test('pack who metadata rejects malformed, duplicate and out-of-range character numbers',()=>{
 for(const who of ['2,2','0,4','33','2.5','2,no','']){
  const p={dasiwa_minimax_h3_reference_pack:true,items:[{type:'image',value:'a.png',forge_label:'character-2',forge_kinds:'character',forge_who:who}]};
  const read=fromPack(p,'/env/in')!;
  expect(read.invalid).toBe(1);expect(read.adds[0]).not.toHaveProperty('who');
 }
});

test('people boxes reach Forge and Director packs, reserve their identities and reject malformed geometry',()=>{
 const boxes=[{n:4,box:[0.1,0.2,0.4,0.8] as [number,number,number,number]},{n:2,box:[0.6,0.2,0.9,0.8] as [number,number,number,number]}];
 const refs=[refAt('pair.png','image',{label:'character-1',boxes}),refAt('auto.png','image')];
 const ctx=h3ForgeContext({pipeline:'ref2v',media:null,lastFrame:null,refs,sourceAudio:false,seconds:5,canvas:{width:64,height:64},inputRoot:'/env/in'});
 expect(ctx.references[0]).toMatchObject({boxes});
 expect(forgeLabels(refs)).toEqual(['character-1','character-1']); // only boxed identities 2 and 4 are reserved
 const {pack}=toPack(refs,'/env/in');
 expect(pack.items[0]).toHaveProperty('forge_boxes',JSON.stringify(boxes));
 expect(fromPack(pack,'/env/in')!.adds[0]).toMatchObject({boxes});
 for(const bad of [[{n:1,box:[0,0,0.01,1]}],[{n:1,box:[0,0,2,1]}],[{n:1,box:[0.5,0,0.1,1]}],[boxes[0],boxes[0]]]){
  const loaded=fromPack({...pack,items:[{...pack.items[0],forge_boxes:JSON.stringify(bad)}]},'/env/in')!;
  expect(loaded.invalid).toBe(1);expect(loaded.adds[0]).not.toHaveProperty('boxes');
 }
});

test('manual people boxes can be added, moved by keyboard, renumbered and removed; image failure disables editing',async()=>{
 const onChange=vi.fn();let refs=[refAt('pair.png','image',{label:'character-1'})];
 const view=()=> <I18nProvider><ReferencesPanel refs={refs} onChange={onChange} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()}/></I18nProvider>;
 const {rerender}=render(view());
 expect(document.querySelector('[data-people-image]')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'Edit pair.png'}));fireEvent.click(screen.getByText('People & order'));
 const add=await screen.findByRole('button',{name:'Add person box'});
 expect(add).toHaveProperty('disabled',true);
 fireEvent.load(document.querySelector('[data-people-image]')!);
 fireEvent.click(add);
 expect(onChange.mock.lastCall![0][0].boxes).toEqual([{n:1,box:[0.25,0.25,0.75,0.75]}]);
 refs=onChange.mock.lastCall![0];rerender(view());
 fireEvent.keyDown(screen.getByRole('button',{name:'Move person box 1'}),{key:'ArrowRight'});
 expect(onChange.mock.lastCall![0][0].boxes[0].box).toEqual([0.26,0.25,0.76,0.75]);
 refs=onChange.mock.lastCall![0];rerender(view());
 fireEvent.change(screen.getByLabelText('Character number for box 1'),{target:{value:'4'}});
 expect(onChange.mock.lastCall![0][0].boxes[0].n).toBe(4);
 expect(onChange.mock.lastCall![0][0].label).toBe('character-4');
 refs=onChange.mock.lastCall![0];rerender(view());
 fireEvent.click(screen.getByRole('button',{name:'Remove person box 4'}));
 expect(onChange.mock.lastCall![0][0].boxes).toEqual([]);
 fireEvent.error(document.querySelector('[data-people-image]')!);
 expect(screen.getByRole('alert').textContent).toContain('Could not load');expect(add).toHaveProperty('disabled',true);
});

test('people-image decode errors reach shared native Logs once, without sending the image path',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockClear().mockResolvedValue(undefined);
 mount([refAt('private.png','image',{label:'character-1'})]);
 fireEvent.click(screen.getByRole('button',{name:'Edit private.png'}));fireEvent.click(screen.getByText('People & order'));
 await screen.findByRole('button',{name:'Add person box'});
 fireEvent.error(document.querySelector('[data-people-image]')!);fireEvent.error(document.querySelector('[data-people-image]')!);
 expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_logs_people_image_error')).toEqual([['native_logs_people_image_error']]);
});

test.each(['input','output'])('Find people on %s preserves existing identities and boxes until explicit Apply',async folder=>{
 vi.mocked(invoke).mockClear().mockImplementation(async(c,a)=>{
  if(c==='native_llm_read')return {provider:'ollama',baseUrl:'http://localhost:11434'};
  const route=(a as {route?:string})?.route;
  if(c==='native_forge'&&route==='models')return {models:[{id:'vision:model',label:'Vision fixture'}],errors:{},creativity:[],shot_counts:[],default_shots:'Auto'};
  if(c==='native_forge'&&route==='people')return {people:[{box:[0.1,0.1,0.4,0.9]},{box:[0.6,0.1,0.9,0.9]}]};
  return {cancelled:true};
 });
 const source=refAt('pair.png','image',{label:'character-4',who:[4,2]});
 const refs=[{...source,item:{...source.item,path:`/env/${folder==='input'?'in':'out'}/pair.png`}}],onChange=vi.fn();
 render(<I18nProvider><ReferencesPanel refs={refs} onChange={onChange} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()} inputRoot="/env/in" outputRoot="/env/out" endpoint="http://127.0.0.1:7801" root="/env"/></I18nProvider>);
 expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_forge')).toHaveLength(0);
 fireEvent.click(screen.getByRole('button',{name:'Edit pair.png'}));fireEvent.click(screen.getByText('People & order'));
 await screen.findByRole('option',{name:'Vision fixture'});
 fireEvent.click(screen.getByRole('button',{name:'Find people'}));
 const apply=await screen.findByRole('button',{name:'Use detected boxes (replace current)'});
 expect(onChange).not.toHaveBeenCalled();
 const call=vi.mocked(invoke).mock.calls.find(([,a])=>(a as {route?:string})?.route==='people')![1] as {body:Record<string,unknown>};
 expect(call.body).toMatchObject({model:'vision:model',path:folder==='input'?'pair.png':'pair.png [output]'});expect(call.body.request_id).toMatch(/^[a-f0-9-]{36}$/);
 fireEvent.click(apply);
 expect(onChange.mock.lastCall![0][0].boxes).toEqual([{n:4,box:[0.1,0.1,0.4,0.9]},{n:2,box:[0.6,0.1,0.9,0.9]}]);
});

test('Find people cancellation discards late results and reference edits invalidate candidates',async()=>{
 let resolve!:(v:unknown)=>void;
 vi.mocked(invoke).mockClear().mockImplementation(async(c,a)=>{
  if(c==='native_llm_read')return {provider:'ollama',baseUrl:'http://localhost:11434'};
  const route=(a as {route?:string})?.route;
  if(c==='native_forge'&&route==='models')return {models:[{id:'vision:model',label:'Vision fixture'}],errors:{},creativity:[],shot_counts:[],default_shots:'Auto'};
  if(c==='native_forge'&&route==='people')return new Promise(r=>{resolve=r;});
  return {cancelled:true};
 });
 let refs=[refAt('pair.png','image',{label:'character-1'})];const onChange=vi.fn();
 const view=()=> <I18nProvider><ReferencesPanel refs={refs} onChange={onChange} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()} inputRoot="/env/in" endpoint="http://127.0.0.1:7801" root="/env"/></I18nProvider>;
 const {rerender}=render(view());fireEvent.click(screen.getByRole('button',{name:'Edit pair.png'}));fireEvent.click(screen.getByText('People & order'));await screen.findByRole('option',{name:'Vision fixture'});
 fireEvent.click(screen.getByRole('button',{name:'Find people'}));
 fireEvent.click(await screen.findByRole('button',{name:'Cancel'}));
 expect(vi.mocked(invoke).mock.calls.some(([,a])=>(a as {route?:string})?.route==='cancel')).toBe(true);
 resolve({people:[{box:[0,0,1,1]}]});
 await waitFor(()=>expect(screen.queryByRole('button',{name:'Use detected boxes (replace current)'})).toBeNull());
 fireEvent.click(screen.getByRole('button',{name:'Find people'}));resolve({people:[{box:[0,0,1,1]}]});
 await screen.findByRole('button',{name:'Use detected boxes (replace current)'});
 refs=[{...refs[0],who:[2],label:'character-2'}];rerender(view());
 await waitFor(()=>expect(screen.queryByRole('button',{name:'Use detected boxes (replace current)'})).toBeNull());expect(onChange).not.toHaveBeenCalled();
});

test.each(['empty','vision','busy','busy422'])('Find people %s leaves existing manual boxes unchanged',async outcome=>{
 vi.mocked(invoke).mockClear().mockImplementation(async(c,a)=>{
  if(c==='native_llm_read')return {provider:'ollama',baseUrl:'http://localhost:11434'};
  const route=(a as {route?:string})?.route;
  if(c==='native_forge'&&route==='models')return {models:[{id:'vision:model',label:'Vision fixture'}],errors:{},creativity:[],shot_counts:[],default_shots:'Auto'};
  if(route==='people'){if(outcome==='empty')return {people:[]};throw Error(outcome==='vision'?'422 People detection failed (no_vision)':outcome==='busy422'?'422 People detection failed (busy)':'409 People detection failed (busy)');}
  return {cancelled:true};
 });
 const onChange=vi.fn();
 render(<I18nProvider><ReferencesPanel refs={[refAt('pair.png','image',{label:'character-1',boxes:[{n:1,box:[0,0,1,1]}]})]} onChange={onChange} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()} inputRoot="/env/in" endpoint="http://127.0.0.1:7801" root="/env"/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Edit pair.png'}));fireEvent.click(screen.getByText('People & order'));await screen.findByRole('option',{name:'Vision fixture'});
 fireEvent.click(screen.getByRole('button',{name:'Find people'}));
 if(outcome==='empty')await screen.findByText('No people found. Current boxes are unchanged; use manual boxes instead.');else {const alert=await screen.findByRole('alert');expect(alert.textContent).toBe(outcome==='vision'?'Choose a vision-capable model to find people.':'The backend is busy. Retry after the workflow or Forge finishes.');}
 expect(onChange).not.toHaveBeenCalled();expect(screen.queryByRole('button',{name:'Use detected boxes (replace current)'})).toBeNull();
});

test('closing People & order cancels only its pending detection request',async()=>{
 vi.mocked(invoke).mockClear().mockImplementation(async(c,a)=>{
  if(c==='native_llm_read')return {provider:'ollama',baseUrl:'http://localhost:11434'};
  const route=(a as {route?:string})?.route;
  if(c==='native_forge'&&route==='models')return {models:[{id:'vision:model',label:'Vision fixture'}],errors:{},creativity:[],shot_counts:[],default_shots:'Auto'};
  if(route==='people')return new Promise(()=>{});
  return {cancelled:true};
 });
 render(<I18nProvider><ReferencesPanel refs={[refAt('pair.png','image',{label:'character-1'})]} onChange={vi.fn()} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()} inputRoot="/env/in" endpoint="http://127.0.0.1:7801" root="/env"/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Edit pair.png'}));fireEvent.click(screen.getByText('People & order'));await screen.findByRole('option',{name:'Vision fixture'});fireEvent.click(screen.getByRole('button',{name:'Find people'}));
 const sent=vi.mocked(invoke).mock.calls.find(([,a])=>(a as {route?:string})?.route==='people')![1] as {body:{request_id:string}};
 fireEvent.click(screen.getByText('People & order'));
 await waitFor(()=>expect(vi.mocked(invoke).mock.calls.some(([,a])=>(a as {route?:string;body?:{request_id:string}})?.route==='cancel'&&(a as {body:{request_id:string}}).body.request_id===sent.body.request_id)).toBe(true));
});

test('the panel edits the picture label and instructions',()=>{
 const onChange=vi.fn();
 render(<I18nProvider><ReferencesPanel refs={[refAt('a.png','image')]} onChange={onChange} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Edit a.png'}));
 const select=screen.getByLabelText('Forge label of a.png') as HTMLSelectElement;
 expect(select.value).toBe('character-1');
 expect(screen.getByRole('option',{name:'Character 1'})).toBeTruthy();
 expect(screen.getByRole('option',{name:'Character 32'})).toHaveProperty('value','character-32');
 expect(screen.getByRole('option',{name:'Characters 2 + 1 (left → right)'})).toHaveProperty('value','group-21');
 fireEvent.change(select,{target:{value:'group-21'}});
 expect(onChange.mock.lastCall![0][0]).toMatchObject({label:'group-21'});
 fireEvent.change(select,{target:{value:'style'}});
 expect(onChange.mock.lastCall![0][0]).toMatchObject({label:'style'});
 fireEvent.change(screen.getByLabelText('Forge instructions for a.png'),{target:{value:'the brush strokes'}});
 expect(onChange.mock.lastCall![0][0]).toMatchObject({instructions:'the brush strokes'});
});

test('keep / ignore reach the Forge and packs; the description feeds the prefill only',()=>{
 const refs=[refAt('a.png','image',{keep:' the coat ',drop:'the rain',description:'hero in red'})];
 const ctx=h3ForgeContext({pipeline:'ref2v',media:null,lastFrame:null,refs,sourceAudio:false,seconds:5,canvas:{width:64,height:64},inputRoot:'/env/in'});
 expect(ctx.references[0]).toMatchObject({keep:'the coat',drop:'the rain'});
 expect(ctx.references[0]).not.toHaveProperty('description');
 const {pack}=toPack(refs,'/env/in');
 expect(pack.items[0]).toMatchObject({forge_keep:'the coat',forge_drop:'the rain'});
 expect(pack.items[0]).not.toHaveProperty('prompt');
 expect(fromPack(pack,'/env/in')!.adds[0]).toMatchObject({keep:'the coat',drop:'the rain'});
 expect(prefillItems(refs)).toEqual([{type:'image',slot:0,mediaMode:'video',description:'hero in red'}]);
 const onChange=vi.fn();
 render(<I18nProvider><ReferencesPanel refs={refs} onChange={onChange} family="minimax-h3" seconds={5} label={()=>''} onAdd={vi.fn()} onInsert={vi.fn()}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Edit a.png'}));
 fireEvent.change(screen.getByLabelText('Ignore from a.png'),{target:{value:'the umbrella'}});
 expect(onChange.mock.lastCall![0][0]).toMatchObject({drop:'the umbrella',keep:' the coat '});
});

test('H3 validates implicit trim starts, known source bounds, aggregate stream durations and both-stream counts',()=>{
 expect(refsProblem([refAt('a.wav','audio',{trimEnd:1})])).toBe('studio.run.video.trimInvalid');
 expect(refsProblem([refAt('a.wav','audio',{trimStart:8,sourceDuration:10})])).toBe('');
 expect(refsProblem([refAt('a.wav','audio',{trimStart:8,trimEnd:12,sourceDuration:10})])).toBe('studio.run.video.trimInvalid');
 expect(refsProblem([refAt('a.wav','audio',{sourceDuration:20})])).toBe('studio.run.video.trimInvalid');
 expect(refsProblem([refAt('v.mp4','video',{mediaMode:'video_audio',trimEnd:10}),refAt('a.wav','audio',{trimEnd:6})])).toBe('studio.run.video.trimTotal');
 expect(refsProblem([refAt('v.mp4','video',{mediaMode:'video',trimEnd:10}),refAt('a.wav','audio',{trimEnd:6})])).toBe('');
 const pictures=Array.from({length:9},(_,n)=>refAt(`${n}.png`,'image'));
 expect(refsProblem([...pictures,refAt('v.mp4','video',{mediaMode:'video_audio'}),refAt('a.wav','audio')])).toBe('');
 expect(refsProblem([...pictures,refAt('v.mp4','video',{mediaMode:'video_audio'}),refAt('a.wav','audio'),refAt('b.wav','audio')])).toBe('studio.run.video.refFull');
});
test('Auto aspect takes the first picture or video by slot, skipping sound-only uses',()=>{
 const ref=(name:string,media:MediaItem['media'],extra={})=>({item:item(name,media),second:0,strength:1,mediaMode:'video',...extra}) as VideoRef;
 expect(firstVisualRef([])).toBeUndefined();
 expect(firstVisualRef([ref('a.wav','audio'),ref('b.png','image')])?.item.name).toBe('b.png');
 expect(firstVisualRef([ref('v.mp4','video',{mediaMode:'audio'}),ref('b.png','image')])?.item.name).toBe('b.png');
 expect(firstVisualRef([ref('p1.png','image'),ref('p2.png','image')])?.item.name).toBe('p1.png');
 expect(firstVisualRef([ref('v.mp4','video'),ref('b.png','image')])?.item.name).toBe('v.mp4');
});
