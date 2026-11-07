import { expect, it } from 'vitest';
import { compileRecipe, type DiagnosticImageIntentV1, type RecipeAllowlist } from './execution';

const intent: DiagnosticImageIntentV1 = {recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:'forest',negativePrompt:'',checkpoint:'base.safetensors',seed:42,width:512,height:768,steps:20,cfg:7,sampler:'euler',scheduler:'normal',outputPrefix:'raw/test'};
const allow: RecipeAllowlist = {nodes:['CheckpointLoaderSimple','CLIPTextEncode','EmptyLatentImage','KSampler','VAEDecode','SaveImage','LoadImage','VAEEncode','UNETLoader','CLIPLoader','VAELoader','LoraLoader','ImageScale','ImageUpscaleWithModel','UpscaleModelLoader','CLIPVisionLoader','CLIPVisionEncode','unCLIPConditioning'],checkpoints:['base.safetensors'],samplers:['euler'],schedulers:['normal'],images:['sub/ref.png']};
const id='映画\\My Model.safetensors';

it('preserves exact Windows checkpoint and LoRA IDs, including allowlist identity',()=>{
 const input={...intent,checkpoint:id,loras:[{name:id,strength:1}]};
 const graph=compileRecipe(input,{...allow,checkpoints:[id],loras:[id]}).graph;
 expect(graph['1'].inputs.ckpt_name).toBe(id);
 expect(Object.values(graph).find(n=>n.class_type==='LoraLoader')!.inputs.lora_name).toBe(id);
 expect(()=>compileRecipe(input,{...allow,checkpoints:[id.replace(/\\/g,'/')],loras:[id]})).toThrow(/not in/);
});

it('preserves Windows split-loader and upscale IDs',()=>{
 const graph=compileRecipe({...intent,checkpoint:id,diffusion:{textEncoder:id,vae:id,clipType:'sdxl'},inputImage:'sub/ref.png',denoise:0.5,inputResize:{width:512,height:768,crop:'disabled'},upscaleModel:id,refine:{width:512,height:768,denoise:0.3,upscaleModel:id}},{...allow,nodes:[...allow.nodes,'DEAS_ImagePostprocessGate'],diffusionModels:[id],textEncoders:[id],vaes:[id],clipTypes:['sdxl'],upscaleModels:[id]}).graph;
 for(const [type,key] of [['UNETLoader','unet_name'],['CLIPLoader','clip_name'],['VAELoader','vae_name'],['UpscaleModelLoader','model_name']])expect(Object.values(graph).find(n=>n.class_type===type)!.inputs[key]).toBe(id);
});

it('preserves Windows vision-model IDs but keeps image names slash-only',()=>{
 const imageReference={visionModel:id,images:[{image:'sub/ref.png',strength:1}]};
 const lists={...allow,visionModels:[id],images:['sub/ref.png']};
 const graph=compileRecipe({...intent,imageReference},lists).graph;
 expect(Object.values(graph).find(n=>n.class_type==='CLIPVisionLoader')!.inputs.clip_name).toBe(id);
 for(const input of [{...intent,inputImage:'sub\\ref.png',denoise:0.5},{...intent,imageReference:{...imageReference,images:[{image:'sub\\ref.png',strength:1}]}}])expect(()=>compileRecipe(input,lists)).toThrow();
});

it.each(['..\\model.safetensors','folder\\..\\model.safetensors','C:\\model.safetensors','\\\\server\\share\\model.safetensors','folder\\\\model.safetensors','folder\\.\\model.safetensors','folder\\model\0.safetensors'])('rejects unsafe Windows model identifiers (%j)',checkpoint=>{
 expect(()=>compileRecipe({...intent,checkpoint},{...allow,checkpoints:[checkpoint]})).toThrow(/Unsafe model identifier/);
});
