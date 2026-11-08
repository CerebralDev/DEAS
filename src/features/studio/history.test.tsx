import { beforeEach, expect, test, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { historicalRows, historyInputSelectionContext, historyRestore, readHistory, readHistoryForMedia, type HistoryRecord, type RestoreContext } from './history';
import { defaultVideoOutput } from '../../lib/videoIntent';
import type { JobIntent } from '../../lib/jobs';
import builtinFamilies from '../../families/builtin.json' with { type: 'json' };
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:vi.fn(()=>true)}));
const image={recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:'forest 猫',negativePrompt:'blur',checkpoint:'exact.safetensors',seed:9007199254740000,width:1216,height:832,steps:17,cfg:4.25,sampler:'euler',scheduler:'normal',outputPrefix:'raw/image/exact'} as const;
const record=(intent:JobIntent=image):HistoryRecord=>({id:'history-1',clientId:'client',promptId:'prompt',origin:'studio',appVersion:'0.1.48',admittedAt:10,observedAt:20,revision:3,state:'failed',intent,outputs:[],outputRoot:null,backendOrigin:'http://127.0.0.1:7801',error:null});
const context:RestoreContext={endpoint:'http://127.0.0.1:7801/',inputRoot:'/selected/input',models:[{name:'exact.safetensors',family:'sdxl'},{name:'style.safetensors',family:'sdxl'}],allow:{nodes:['VAEDecodeTiled','EasyCache'],checkpoints:['exact.safetensors'],samplers:['euler'],schedulers:['normal'],images:['first.png','second.png'],loras:['style.safetensors'],visionModels:['vision.safetensors'],attention:['sdpa'],compile:['inductor'],textEncoders:['clip.safetensors'],vaes:['vae.safetensors']}};
beforeEach(()=>{vi.clearAllMocks();vi.mocked(invoke).mockResolvedValue(null);});

test('exact historical output reuse requires unique root/path and native current availability',async()=>{
 const output={filename:'film.mp4',subfolder:'starred/video/date',type:'output' as const};
 const saved={...record(),outputRoot:'/output',outputs:[output]};
 vi.mocked(invoke).mockImplementation(async command=>command==='native_jobs_history'?{schemaVersion:1,records:[saved],readOnly:false,error:null}:command==='native_jobs_history_outputs'?['available']:null);
 expect(await readHistoryForMedia('/output/starred/video/date/film.mp4')).toEqual(saved);
 expect(invoke).toHaveBeenCalledWith('native_jobs_history_outputs',{jobId:saved.id});
 vi.mocked(invoke).mockClear();
 expect(await readHistoryForMedia('/other/starred/video/date/film.mp4')).toBeNull();
 expect(invoke).not.toHaveBeenCalledWith('native_jobs_history_outputs',expect.anything());
 vi.mocked(invoke).mockImplementation(async command=>command==='native_jobs_history'?{schemaVersion:1,records:[saved,{...saved,id:'another-job'}],readOnly:false,error:null}:['available']);
 expect(await readHistoryForMedia('/output/starred/video/date/film.mp4')).toBeNull();
 for(const availability of ['missing','unvalidated']){
  vi.mocked(invoke).mockImplementation(async command=>command==='native_jobs_history'?{schemaVersion:1,records:[saved],readOnly:false,error:null}:[availability]);
  expect(await readHistoryForMedia('/output/starred/video/date/film.mp4')).toBeNull();
 }
});
test.each(['ltx-2','minimax-h3'] as const)('history retains exact latent post choice and discloses missing model/node (%s)',family=>{
 const {imageFormat,imageCompression,...output}=defaultVideoOutput;void imageFormat;void imageCompression;
 const video:JobIntent={recipeId:'video-generation',version:1,positivePrompt:'film',negativePrompt:'',model:'model',textEncoder:'text',vae:'v',seed:7,width:768,height:512,frames:family==='ltx-2'?121:124,frameRate:24,steps:8,cfg:1,sampler:'euler',scheduler:'simple',outputPrefix:'raw/video/7',output,...(family==='ltx-2'?{family,ltx:{passes:1,audioVae:'audio',textProjection:'projection'},latentUpscale:{model:'missing.safetensors',scale:2}}:{family,h3:{mode:'T2VA',audioVae:'audio',shiftVideo:10,shiftAudio:3,references:[]},latentUpscale:{model:'missing.safetensors',scale:2,steps:1,denoise:0.2}})};
 const before=JSON.stringify(video),restored=historyRestore(record(video),context);
 expect(restored.intent).toEqual(video);
 expect(restored.unavailable).toContain('latentUpscale: missing.safetensors');
 expect(restored.unavailable).toContain(family==='ltx-2'?'LTXVLatentUpsampler':'DaSiWaH3TiledUpscale');
 expect(JSON.stringify(video)).toBe(before);expect(invoke).not.toHaveBeenCalled();
});

test('history mapper preserves immutable exact seed, canvas, options, LoRAs and ordered ReVision strengths',()=>{
 const original=record({...image,loras:[{name:'style.safetensors',strength:0.75}],options:{attention:'sdpa',cache:{node:'EasyCache',threshold:0.03},compile:'inductor',vaeTile:1536},imageReference:{visionModel:'vision.safetensors',images:[{image:'second.png',strength:0.2},{image:'first.png',strength:0.9}]},imageFormat:'webp',imageCompression:83});
 const before=JSON.stringify(original),restore=historyRestore(original,context);
 expect(restore.unavailable).toEqual(['input: second.png','input: first.png']);expect(restore.unsupported).toEqual([]);
 expect(restore.intent).toEqual(original.intent);expect(restore.intent).not.toBe(original.intent);
 expect(restore.options).toMatchObject({attention:'a:sdpa',cache:'EasyCache',threshold:0.03,compile:'inductor',vae:'tiled',tile:1536,outputNaming:'image/exact',output:{imageFormat:'webp',imageCompression:83}});
 expect(restore.references).toEqual([{name:'second.png',strength:0.2},{name:'first.png',strength:0.9}]);
 restore.references.reverse();expect(JSON.stringify(original)).toBe(before);
});
test.each(['qwen-image-2.1','krea-2','moss-image'] as const)('native reference mapper retains method/order without inventing strengths for %s',method=>{
 const restored=historyRestore(record({...image,nativeImageReference:{method,images:['second.png','first.png']}}),{...context,models:[{name:image.checkpoint,family:method}]});
 expect(restored.intent).toMatchObject({nativeImageReference:{method,images:['second.png','first.png']}});
 expect(restored.references.map(r=>r.name)).toEqual(['second.png','first.png']);
 expect(restored.references.every(r=>!r.path)).toBe(true);
 expect(restored.unavailable).toEqual(expect.arrayContaining(['input: second.png','input: first.png']));
});
test('missing exact model and media are disclosed; other environment never resolves current file names',()=>{
 const restored=historyRestore(record({...image,checkpoint:'missing/exact.safetensors',inputImage:'first.png',inputMask:'mask.png',inputLayers:[{image:'layer.png',x:1,y:2,width:64,height:64}],inputResize:{width:1216,height:832,crop:'center'}}),{...context,endpoint:'http://127.0.0.1:9999/'});
 expect(restored.environmentMismatch).toBe(true);expect(restored.source).toEqual({name:'first.png',strength:1});
 expect(restored.unavailable).toEqual(expect.arrayContaining(['model: missing/exact.safetensors','input: first.png','backendOrigin']));
 expect(restored.unsupported).toEqual(expect.arrayContaining(['inputMask','inputLayers','inputResize']));
 expect(restored.pipeline).toBe('i2i');
});
test('reference path traversal and unlisted uploaded handles remain unresolved',()=>{
 const restored=historyRestore(record({...image,imageReference:{visionModel:'vision.safetensors',images:[{image:'../foreign.png',strength:0.6},{image:'deas-inputs/upload.png',strength:0.4}]}}),{...context,allow:{...context.allow!,images:['../foreign.png']}});
 expect(restored.references.every(r=>!r.path)).toBe(true);
 expect(restored.unavailable).toContain('input: deas-inputs/upload.png');
});
test('valid diagnostic input dimensions that Studio would snap are disclosed rather than silently rescaled',()=>{
 const restored=historyRestore(record({...image,width:1000,height:1000,inputImage:'first.png',denoise:0.6}),context);
 expect(restored.unsupported).toContain('effective dimensions');expect(restored.intent).toMatchObject({width:1000,height:1000});
});
test('video family changes cannot silently rebuild a different family recipe',()=>{
 const {imageFormat,imageCompression,...output}=defaultVideoOutput;void imageFormat;void imageCompression;
 const restored=historyRestore(record({recipeId:'video-generation',version:1,family:'ltx-2',positivePrompt:'video',negativePrompt:'',model:image.checkpoint,textEncoder:'clip.safetensors',vae:'vae.safetensors',seed:43,width:768,height:512,frames:49,frameRate:24,steps:20,cfg:5,sampler:'euler',scheduler:'normal',outputPrefix:'raw/video/exact',output,ltx:{passes:1,audioVae:'vae.safetensors',textProjection:'clip.safetensors'}}),{...context,models:[{name:image.checkpoint,family:'minimax-h3'}]});
 expect(restored.unavailable).toContain('video family: ltx-2');expect(restored.intent).toMatchObject({family:'ltx-2'});
});
test('representable refine preserves exact pass; anisotropic geometry is disclosed before partial apply',()=>{
 const restored=historyRestore(record({...image,refine:{width:2432,height:1664,denoise:0.35,tiled:{tileWidth:1024,tileHeight:1024,overlap:128}}}),context);
 expect(restored.unsupported).toEqual([]);
 expect(historyRestore(record({...image,refine:{width:2432,height:2048,denoise:0.35}}),context).unsupported).toContain('refine.dimensions');
});
 test('video mapper discloses components and Continuity without reconstructing a session',()=>{
 const {imageFormat,imageCompression,...output}=defaultVideoOutput;void imageFormat;void imageCompression;
 const restored=historyRestore(record({recipeId:'video-generation',version:1,family:'minimax-h3',positivePrompt:'video',negativePrompt:'',model:'h3.safetensors',textEncoder:'clip.safetensors',vae:'vae.safetensors',seed:43,width:768,height:512,frames:124,frameRate:24,steps:20,cfg:5,sampler:'euler',scheduler:'normal',outputPrefix:'raw/video/exact',output,h3:{mode:'I2VA',audioVae:'missing-audio.safetensors',shiftVideo:10,shiftAudio:3,references:[{slot:0,image:'first.png'}],continuity:{session:'secret-session',sourceId:'source',capture:true,action:'next',outputMode:'segment'}}}),context);
 expect(restored.pipeline).toBe('i2v');expect(restored.source).toEqual({name:'first.png',strength:1});
 expect(restored.unavailable).toContain('vae: missing-audio.safetensors');expect(restored.unsupported).toContain('h3.continuity');
});
test('REF2VA media files restore as named re-pick inputs in Director lane order instead of blocking the draft',()=>{
 const {imageFormat,imageCompression,...output}=defaultVideoOutput;void imageFormat;void imageCompression;
 const restored=historyRestore(record({recipeId:'video-generation',version:1,family:'minimax-h3',positivePrompt:'video',negativePrompt:'',model:'h3.safetensors',textEncoder:'clip.safetensors',vae:'vae.safetensors',seed:43,width:768,height:512,frames:124,frameRate:24,steps:20,cfg:5,sampler:'euler',scheduler:'normal',outputPrefix:'raw/video/exact',output,h3:{mode:'REF2VA',audioVae:'vae.safetensors',shiftVideo:10,shiftAudio:3,references:[],media:[
  {type:'audio',file:'song.mp3',slot:0},
  {type:'video',file:'motion.mp4',slot:1,mediaMode:'video_audio',trimStart:1.5,trimEnd:9.5},
  {type:'video',file:'clip.mp4',slot:0},
  {type:'image',file:'face.png',slot:2},
 ]}}),context);
 expect(restored.pipeline).toBe('ref2v');expect(restored.unsupported).toEqual([]);
 expect(restored.videoRefs).toEqual([
  {name:'face.png',type:'image',strength:1},
  {name:'clip.mp4',type:'video',strength:1},
  {name:'motion.mp4',type:'video',strength:1,mediaMode:'video_audio',trimStart:1.5,trimEnd:9.5},
  {name:'song.mp3',type:'audio',strength:1},
 ]);
 expect(historyRestore(record({recipeId:'video-generation',version:1,family:'minimax-h3',positivePrompt:'video',negativePrompt:'',model:'h3.safetensors',textEncoder:'clip.safetensors',vae:'vae.safetensors',seed:43,width:768,height:512,frames:124,frameRate:24,steps:20,cfg:5,sampler:'euler',scheduler:'normal',outputPrefix:'raw/video/exact',output,h3:{mode:'REF2VA',audioVae:'vae.safetensors',shiftVideo:10,shiftAudio:3,references:[{slot:1,image:'last.png'}]}}),context).unsupported).toContain('h3.references');
});
test.each([
 {endpoint:'http://127.0.0.1:7801/',inputRoot:'/environment-B/input',outputRoot:'/environment-A/output'},
 {endpoint:'http://127.0.0.1:7801/backend-B/',inputRoot:'/backend-B/input',outputRoot:'/backend-B/output'},
 {endpoint:'http://127.0.0.1:7801/',inputRoot:'/selected/input',outputRoot:'/selected/output'},
])('history never binds same-named input or reference by current namespace/root/output provenance ($endpoint, $inputRoot)',({endpoint,inputRoot,outputRoot})=>{
 const current={...context,endpoint,inputRoot,allow:{...context.allow!,images:['same.png','reference.png']}};
 for(const reference of [
  {imageReference:{visionModel:'vision.safetensors',images:[{image:'reference.png',strength:0.3},{image:'same.png',strength:0.8}]}},
  {nativeImageReference:{method:'qwen-image-2.1' as const,images:['reference.png','same.png']}},
 ]){
  const historical={...record({...image,inputImage:'same.png',denoise:0.6,...reference}),outputRoot,outputRootIdentity:'known-output-root-only'};
  const before=JSON.stringify(historical),restored=historyRestore(historical,current);
  expect(restored.source).toEqual({name:'same.png',strength:1});
  expect(restored.references.map(r=>r.name)).toEqual(['reference.png','same.png']);
  expect(restored.references.every(r=>r.path===undefined)).toBe(true);
  expect(restored.unavailable).toEqual(expect.arrayContaining(['input: same.png','input: reference.png']));
  expect(JSON.stringify(historical)).toBe(before);expect(invoke).not.toHaveBeenCalled();
 }
});
test('legacy video frame handles remain unresolved even in a matching current input listing',()=>{
 const {imageFormat,imageCompression,...output}=defaultVideoOutput;void imageFormat;void imageCompression;
 const restored=historyRestore(record({recipeId:'video-generation',version:1,family:'ltx-2',positivePrompt:'video',negativePrompt:'',model:image.checkpoint,textEncoder:'clip.safetensors',vae:'vae.safetensors',seed:43,width:768,height:512,frames:49,frameRate:24,steps:20,cfg:5,sampler:'euler',scheduler:'normal',outputPrefix:'raw/video/exact',output,firstFrame:'first.png',lastFrame:'second.png',ltx:{passes:1,audioVae:'vae.safetensors',textProjection:'clip.safetensors'}}),context);
 expect(restored.source).toEqual({name:'first.png',strength:1});expect(restored.lastFrame).toEqual({name:'second.png',strength:1});
 expect(restored.unavailable).toEqual(expect.arrayContaining(['input: first.png','input: second.png']));expect(invoke).not.toHaveBeenCalled();
});
test('explicit-selection scopes distinguish backend prefixes/input roots and omit credentials/query/hash secrets',()=>{
 const clean=historyInputSelectionContext('http://127.0.0.1:7801/backend-A/','C:/env/input');
 const credentialed=historyInputSelectionContext('http://user:password@127.0.0.1:7801/backend-A/?token=secret#private','C:\\env\\input\\');
 expect(credentialed).toBe(clean);expect(credentialed).not.toMatch(/password|secret|private|user/);
 expect(historyInputSelectionContext('http://127.0.0.1:7801/backend-B/','C:/env/input')).not.toBe(clean);
 expect(historyInputSelectionContext('http://127.0.0.1:7801/backend-A/','C:/other/input')).not.toBe(clean);
});
test.each(['ltx-2','minimax-h3'] as const)('current %s text-to-video intent has no projection warning with unchanged capabilities',family=>{
 const {imageFormat,imageCompression,...output}=defaultVideoOutput;void imageFormat;void imageCompression;
 const intent:JobIntent={recipeId:'video-generation',version:1,positivePrompt:'film',negativePrompt:'',model:'video.safetensors',textEncoder:'clip.safetensors',vae:'vae.safetensors',seed:7,width:768,height:512,frames:family==='ltx-2'?121:124,frameRate:24,steps:8,cfg:1,sampler:'euler',scheduler:'normal',outputPrefix:'raw/video/7',output,...(family==='ltx-2'?{family,ltx:{passes:1,audioVae:'vae.safetensors',textProjection:'clip.safetensors'}}:{family,h3:{mode:'T2VA',audioVae:'vae.safetensors',shiftVideo:10,shiftAudio:3,references:[]}})};
 const original={...record(intent),state:'succeeded' as const},before=JSON.stringify(original);
 const restored=historyRestore(original,{...context,models:[{name:'video.safetensors',family}]});
 expect(restored.pipeline).toBe('t2v');expect(restored.unsupported).toEqual([]);expect(restored.unavailable).toEqual([]);
 expect(restored.intent).toEqual(intent);expect(JSON.stringify(original)).toBe(before);expect(invoke).not.toHaveBeenCalled();
});
test('history accepts the same declared LoRA family as Studio without accepting missing or unrelated LoRAs',()=>{
 const family=builtinFamilies.find(f=>f.id==='qwen-image-2.1-turbo')!;
 const intent={...image,sigmas:family.sigmas,diffusion:{textEncoder:'clip.safetensors',vae:'vae.safetensors',clipType:family.clipType!},loras:[{name:'style.safetensors',strength:0.75}]};
 const current:RestoreContext={...context,models:[{name:image.checkpoint,family:family.id},{name:'style.safetensors',family:'qwen-image-2.1'}]};
 const original={...record(intent),state:'succeeded' as const},before=JSON.stringify(original);
 const restored=historyRestore(original,current);
 expect(restored.unavailable).toEqual([]);expect(restored.unsupported).toEqual([]);
 expect(restored.intent).toEqual(intent);expect(JSON.stringify(original)).toBe(before);expect(invoke).not.toHaveBeenCalled();
 expect(historyRestore(original,{...current,models:[current.models[0],{name:'style.safetensors',family:'sdxl'}]}).unavailable).toContain('LoRA: style.safetensors');
 expect(historyRestore(original,{...current,models:[current.models[0]]}).unavailable).toContain('LoRA: style.safetensors');
 expect(historyRestore(original,{...current,allow:{...current.allow!,loras:[]}}).unavailable).toContain('LoRA: style.safetensors');
});
test('absent effective sigmas cannot silently pick a family default',()=>{
 const restored=historyRestore(record(image),{...context,models:[{name:image.checkpoint,family:'qwen-image-2.1-turbo'}]});
 expect(restored.unsupported).toContain('model default sigmas');
});
test('deduplication changes only history presentation',()=>{
 const records=[record(),{...record(),id:'older'}];expect(historicalRows([...records,records[0]]).map(r=>r.id)).toEqual(['history-1','older']);expect(records).toHaveLength(2);
});
test('history facade is one arg-free metadata IPC and reports malformed response via fixed shared Logs message',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockResolvedValueOnce({schemaVersion:1,records:[record()],readOnly:false,error:null});
 await readHistory();expect(invoke).toHaveBeenCalledExactlyOnceWith('native_jobs_history',{});
 vi.mocked(invoke).mockResolvedValueOnce({schemaVersion:99});await expect(readHistory()).rejects.toThrow('Invalid native history response');
 expect(invoke).toHaveBeenLastCalledWith('native_logs_history_error');
});

const music={recipeId:'music-generation',version:1,family:'yue2',mode:'cover',planning:'melody',checkpoint:'yue2.safetensors',style:'style',lyrics:'',seed:1,maxDuration:360,steps:32,cfg:1,sampler:'dpm_2',scheduler:'sgm_uniform',generation:{temperature:1,topP:0.95,topK:100,repetitionPenalty:1.2,cfgScale:1},outputPrefix:'raw/audio/x',sourceAudio:'deas-input-old.wav',audioEncoder:'sheetsage2_bf16.safetensors'} as const;
test('music history restores to its own pipeline, names the unresolved source and discloses missing nodes/encoder/family by name',()=>{
 const musicContext:RestoreContext={...context,models:[{name:'yue2.safetensors',family:'yue2'}],allow:{...context.allow!,nodes:['YuE2GenerateMusic','EmptyYuE2LatentAudio','VAEDecodeAudio','SaveAudioAdvanced','LoadAudio','AudioEncoderLoader','SheetSage2AudioToABC'],samplers:['dpm_2'],schedulers:['sgm_uniform'],audioEncoders:['sheetsage2_bf16.safetensors']}};
 const ok=historyRestore(record(music),musicContext);
 expect(ok.pipeline).toBe('cover');expect(ok.source).toEqual({name:'deas-input-old.wav',strength:1});
 expect(ok.unsupported).toEqual([]);expect(ok.unavailable).toEqual(['input: deas-input-old.wav']);
 expect(ok.options.sigmas).toBe('');expect(ok.options.output?.imageFormat).toBe('png');expect(ok.references).toEqual([]);
 const t2m=historyRestore(record({...music,mode:'t2m',planning:'off',sourceAudio:undefined,audioEncoder:undefined} as unknown as JobIntent),musicContext);
 expect(t2m.pipeline).toBe('t2m');expect(t2m.source).toBeUndefined();expect(t2m.unavailable).toEqual([]);
 const missing=historyRestore(record(music),{...musicContext,models:[{name:'yue2.safetensors',family:'sdxl'}],allow:{...musicContext.allow!,nodes:['YuE2GenerateMusic'],audioEncoders:[]}});
 expect(missing.unavailable).toEqual(expect.arrayContaining(['EmptyYuE2LatentAudio','SheetSage2AudioToABC','audioEncoder: sheetsage2_bf16.safetensors','family: yue2','input: deas-input-old.wav']));
 expect(missing.unavailable).not.toContain('YuE2GenerateMusic');expect(missing.unavailable).not.toContain('YuE2GenerateABC');
});
test('a saved tiled refine on a temporal-latent family is named, not silently dropped',()=>{
 const restored=historyRestore(record({...image,refine:{width:2432,height:1664,denoise:0.35,tiled:{tileWidth:1024,tileHeight:1024,overlap:128}}}),{...context,models:[{name:'exact.safetensors',family:'anima'}]});
 expect(restored.unsupported).toContain('refine.tiled');
});

const aceIntent={recipeId:'music-generation',version:1,family:'ace-step-1-5-xl-sft',mode:'t2m',planning:'off',checkpoint:'ace-xl.safetensors',style:'jazz 猫',lyrics:'',seed:42,maxDuration:30,steps:37,cfg:4.25,sampler:'euler',scheduler:'simple',outputPrefix:'raw/audio/exact',ace:{shift:2.5,lyricsStrength:1,bpm:123,timeSignature:'3',language:'de',keyScale:'D minor',generateAudioCodes:false,lmCfg:3.5,temperature:0.7,topP:0.8,topK:12,minP:0.03},diffusion:{textEncoder:'text.safetensors',textEncoder2:'code.safetensors',vae:'audio-vae.safetensors'},audioEnhance:{mode:'clean',profile:'mix'}} as const;

const aceContext:RestoreContext={...context,models:[{name:aceIntent.checkpoint,family:aceIntent.family}],allow:{...context.allow!,nodes:['VAEDecodeAudio','SaveAudioAdvanced','KSampler','ConditioningZeroOut','TextEncodeAceStepAudio1.5','EmptyAceStep1.5LatentAudio','ModelSamplingAuraFlow','UNETLoader','DualCLIPLoader','VAELoader'],samplers:['euler'],schedulers:['simple'],dualClipTypes:['ace'],dualTextEncoders:['text.safetensors','code.safetensors'],vaes:['audio-vae.safetensors']}};
test('ACE restore preserves exact submitted family, sampling, settings and split components without YuE2 dependencies',()=>{
 const original=record(aceIntent),before=JSON.stringify(original),restored=historyRestore(original,aceContext);
 expect(restored.pipeline).toBe('t2m');expect(restored.unavailable).toEqual([]);expect(restored.unsupported).toEqual([]);
 expect(restored.source).toBeUndefined();expect(restored.references).toEqual([]);expect(restored.intent).toEqual(aceIntent);expect(restored.intent).not.toBe(original.intent);
 expect(JSON.stringify(original)).toBe(before);expect(invoke).not.toHaveBeenCalled();
 const missing=historyRestore(original,{...aceContext,models:[{name:aceIntent.checkpoint,family:'ace-step-1-5-xl-turbo'}],allow:{...aceContext.allow!,nodes:[],dualTextEncoders:[],dualClipTypes:[],vaes:[]}});
 expect(missing.unavailable).toEqual(expect.arrayContaining(['family: ace-step-1-5-xl-sft','textEncoder: text.safetensors','textEncoder: code.safetensors','vae: audio-vae.safetensors','TextEncodeAceStepAudio1.5']));
 expect(missing.unavailable.join(' ')).not.toMatch(/YuE2|SheetSage/);
});
test.each([
 {family:'ace-step-1-5'},{family:'ace-step-1-5-xl'},{mode:'cover'},{planning:'full'},{abc:{}},{generation:{}},{sourceAudio:'old.wav'},{audioEncoder:'old.safetensors'},{options:{}},{loras:[]},{ace:undefined},{ace:{...aceIntent.ace,topK:-1}},{ace:{...aceIntent.ace,language:'invalid'}},{maxDuration:30.01},{seed:NaN},{cfg:Infinity},{steps:0},{outputPrefix:null},{diffusion:{...aceIntent.diffusion,textEncoder2:undefined}},{diffusion:{...aceIntent.diffusion,vae:''}},{diffusion:null},{diffusion:{...aceIntent.diffusion,textEncoder2:aceIntent.diffusion.textEncoder}},{unexpected:true},{outputPrefix:'../outside'},{family:'yue2'},{audioEnhance:null},{audioEnhance:{mode:'off',profile:'mix'}},
])('malformed or incompatible ACE history is blocked without coercion: %j',invalid=>{
 const intent={...aceIntent,...invalid} as unknown as JobIntent,before=structuredClone(intent);
 const restored=historyRestore(record(intent),aceContext);
 expect(restored.unsupported).toContain('music intent');expect(restored.intent).toEqual(before);expect(intent).toEqual(before);expect(invoke).not.toHaveBeenCalled();
});
test.each(['ace-step-1-5-base','ace-step-1-5-sft','ace-step-1-5-turbo','ace-step-1-5-xl-base','ace-step-1-5-xl-sft','ace-step-1-5-xl-turbo'] as const)('ACE history retains exact %s variant and explicit nondefault values',family=>{
 const intent={...aceIntent,family},restored=historyRestore(record(intent),{...aceContext,models:[{name:intent.checkpoint,family}]});
 expect(restored.unsupported).toEqual([]);expect(restored.unavailable).toEqual([]);expect(restored.intent).toEqual(intent);
});
test('original ACE AIO restores without assuming 1.5 components or planning',()=>{
 const intent={...aceIntent,family:'ace-step-1-0',maxDuration:4096/44100*330,diffusion:undefined} as const;
 const restored=historyRestore(record(intent),{...aceContext,models:[{name:intent.checkpoint,family:intent.family}],allow:{...aceContext.allow!,nodes:['VAEDecodeAudio','SaveAudioAdvanced','KSampler','ConditioningZeroOut','TextEncodeAceStepAudio','EmptyAceStepLatentAudio','ModelSamplingSD3','LatentOperationTonemapReinhard','LatentApplyOperationCFG','CheckpointLoaderSimple']}});
 expect(restored.unsupported).toEqual([]);expect(restored.unavailable).toEqual([]);expect(restored.intent).toEqual(intent);
 expect(historyRestore(record({...intent,diffusion:aceIntent.diffusion}),aceContext).unsupported).toContain('music intent');
});
