import { expect, test, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { comfySampler, generationSettings, imageMetadataRows, readImageMetadata } from './imageMetadata';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn()}));
test('Swarm metadata stored under parameters becomes readable rows without losing extra fields or literal text',()=>{
 expect(imageMetadataRows([['parameters',JSON.stringify({sui_image_params:{prompt:'猫 <script>not markup</script>',model:'Krea2/model',seed:0,cfgscale:1,automaticvae:true},sui_extra_data:{generation_time:'13.31 sec gen',date:'2026-10-16'}})]]))
 .toEqual([['prompt','猫 <script>not markup</script>'],['model','Krea2/model'],['seed','0'],['cfgscale','1'],['automaticvae','true'],['generation_time','13.31 sec gen'],['date','2026-10-16']]);
});
test('ComfyUI graph displays actual node inputs, not graph links or editor coordinates',()=>{
 const graph={'1':{class_type:'CheckpointLoaderSimple',inputs:{ckpt_name:'m.safetensors'}},'2':{class_type:'CLIPTextEncode',inputs:{clip:['1',1],text:'forest'}},'3':{class_type:'KSampler',inputs:{seed:123,steps:8,positive:['2',0]}}};
 expect(imageMetadataRows([['prompt',JSON.stringify(graph)],['workflow','editor positions']])).toEqual([['CheckpointLoaderSimple #1 · ckpt_name','m.safetensors'],['CLIPTextEncode #2 · text','forest'],['KSampler #3 · seed','123'],['KSampler #3 · steps','8']]);
});
test('A1111, malformed JSON and unknown fields remain intact instead of guessed settings',()=>{
 const entries:[string,string][]=[['parameters','forest\nNegative prompt: blur\nSteps: 8, Seed: 123'],['Comment','{broken'],['Description','ordinary picture']];
 expect(imageMetadataRows(entries)).toEqual(entries);expect(imageMetadataRows([])).toEqual([]);
});
test('reusable settings come from A1111 text (LoRA tags lifted), Swarm params or a ComfyUI graph; otherwise null',()=>{
 const a1111='a cat, Grüße, <lora:style:0.8>\nNegative prompt: blur, low\nSteps: 20, Sampler: euler karras, CFG scale: 7.0, Seed: 123, Model hash: 44670e67c5, Lora hashes: "style: 1234567890", Size: 512x768, Model: Illustrious/m.safetensors';
 expect(generationSettings([['prompt','{"7":{"class_type":"SaveImage","inputs":{}}}'],['parameters',a1111]])).toEqual({positive:'a cat, Grüße',negative:'blur, low',checkpoint:'Illustrious/m.safetensors',seed:123,steps:20,cfg:7,sampler:'euler',scheduler:'karras',width:512,height:768,loras:[{name:'style',strength:0.8}]});
 expect(generationSettings([['parameters',JSON.stringify({sui_image_params:{prompt:'swarm',negativeprompt:'',model:'Krea2/model',seed:'7',steps:8,cfgscale:'1',sampler:'euler',scheduler:'simple',width:896,height:1152,loras:'a, b',loraweights:'0.5,1'}})]]))
  .toEqual({positive:'swarm',checkpoint:'Krea2/model',seed:7,steps:8,cfg:1,sampler:'euler',scheduler:'simple',width:896,height:1152,loras:[{name:'a',strength:0.5},{name:'b',strength:1}]});
 const graph={'1':{class_type:'CheckpointLoaderSimple',inputs:{ckpt_name:'m.safetensors'}},'2':{class_type:'CLIPTextEncode',inputs:{clip:['L1',1],text:'forest'}},'3':{class_type:'CLIPTextEncode',inputs:{clip:['L1',1],text:'blur'}},'4':{class_type:'EmptyLatentImage',inputs:{width:512,height:768,batch_size:1}},'L1':{class_type:'LoraLoader',inputs:{model:['1',0],clip:['1',1],lora_name:'style/a.safetensors',strength_model:0.8,strength_clip:0.8}},'5':{class_type:'KSampler',inputs:{model:['L1',0],positive:['2',0],negative:['3',0],latent_image:['4',0],seed:123,steps:8,cfg:7,sampler_name:'euler',scheduler:'normal',denoise:1}}};
 expect(generationSettings([['prompt',JSON.stringify(graph)],['workflow','{}']])).toEqual({positive:'forest',negative:'blur',checkpoint:'m.safetensors',seed:123,steps:8,cfg:7,sampler:'euler',scheduler:'normal',width:512,height:768,loras:[{name:'style/a.safetensors',strength:0.8}]});
 expect(generationSettings([['Comment','{broken'],['Description','ordinary picture'],['parameters','no details line']])).toBeNull();
 expect(generationSettings([['prompt','{"1":{"class_type":"SaveImage","inputs":{}}}']])).toBeNull();
});
test('DaSiWa saver `*_not_found` sentinels stay absent: an empty negative prompt does not fill the field',()=>{
 const meta='a cat\nNegative prompt: negative_prompt_not_found\nSteps: 20, Sampler: euler karras, CFG scale: 7.0, Seed: 123, Size: 512x768, Model: model_not_found.safetensors';
 expect(generationSettings([['parameters',meta]])).toEqual({positive:'a cat',seed:123,steps:20,cfg:7,sampler:'euler',scheduler:'karras',width:512,height:768});
 expect(generationSettings([['parameters','a cat\nNegative prompt: real_not_found words\nSteps: 20, Seed: 123']])).toEqual({positive:'a cat',negative:'real_not_found words',seed:123,steps:20});
});
test('A1111 sampler names map to ComfyUI sampler + scheduler; unknown ones are reported, not guessed',()=>{
 expect(comfySampler('Euler a')).toEqual({sampler:'euler_ancestral'});
 expect(comfySampler('DPM++ 2M Karras')).toEqual({sampler:'dpmpp_2m',scheduler:'karras'});
 expect(comfySampler('DPM++ 2M SDE Exponential')).toEqual({sampler:'dpmpp_2m_sde',scheduler:'exponential'});
 expect(comfySampler('euler karras')).toEqual({sampler:'euler',scheduler:'karras'});
 expect(comfySampler('dpmpp_3m_sde_gpu')).toEqual({sampler:'dpmpp_3m_sde_gpu'});
 for(const bad of ['PLMS','Euler weird','']){expect(comfySampler(bad)).toBeNull();}
 const s=generationSettings([['parameters','x\nSteps: 1, Sampler: PLMS, Schedule type: Automatic, Seed: 2']]);
 expect(s).toEqual({positive:'x',steps:1,seed:2,scheduler:'normal',skipped:['Sampler: PLMS']});
 expect(generationSettings([['parameters','x\nSteps: 1, Sampler: DPM++ 2M, Schedule type: Karras, Seed: 2']])).toMatchObject({sampler:'dpmpp_2m',scheduler:'karras'});
});
test('native metadata IPC validates shape and size before display',async()=>{
 vi.mocked(invoke).mockResolvedValue([['parameters','abc']]);expect(await readImageMetadata('/images/a.png')).toEqual([['parameters','abc']]);
 expect(invoke).toHaveBeenCalledWith('native_media_metadata',{path:'/images/a.png'});
 for(const bad of [{},[[1,'x']],[['x']],Array.from({length:65},()=>['a','b']),[['x','x'.repeat(2*1024*1024)]]]){
  vi.mocked(invoke).mockResolvedValue(bad);await expect(readImageMetadata('/a')).rejects.toThrow('Invalid image metadata');
 }
});
