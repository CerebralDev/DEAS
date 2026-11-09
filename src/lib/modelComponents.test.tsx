import { expect, test } from 'vitest';
import families from '../families/builtin.json' with { type: 'json' };
import { componentFiles, defaultComponentOverrides, isComponentOverrides } from './modelComponents';
import type { LocalModel } from './models';
const model=(category:string,name:string,component?:string):LocalModel=>({category,name,component,path:'/models/'+category+'/'+name,size:1,info:null});
const family={textEncoders:['qwen_3_600m'],vaes:['wan_2.1_vae'],components:{textEncoders:['qwen3-0.6b'],vaes:['wan-2.1-vae']}};
test('header compatibility survives renaming and rejects a misleading name with a different architecture',()=>{
 const files=[model('text_encoders','dasiwaAnima_luminousLabyrinthV1_txt.safetensors','qwen3-0.6b'),model('clip','qwen_3_600m-wrong.safetensors','qwen3-vl-8b'),model('vae','vae.safetensors','wan-2.1-vae')];
 expect(componentFiles(files,family,'textEncoders')).toEqual(['dasiwaAnima_luminousLabyrinthV1_txt.safetensors']);
 expect(componentFiles(files,family,'vaes')).toEqual(['vae.safetensors']);
 expect(componentFiles(files,family,'textEncoders',true)).toHaveLength(2);
 expect(componentFiles(files,family,'vaes',true)).toEqual(['vae.safetensors']);
});
test('unknown formats keep filename hints, while All files is bounded to the slot category',()=>{
 const files=[model('text_encoders','qwen_3_600m.gguf'),model('clip','renamed.safetensors'),model('checkpoints','not-an-encoder.safetensors'),model('vae','wan_2.1_vae.safetensors')];
 expect(componentFiles(files,family,'textEncoders')).toEqual(['qwen_3_600m.gguf']);
 expect(componentFiles(files,family,'textEncoders',true)).toEqual(['qwen_3_600m.gguf','renamed.safetensors']);
 expect(componentFiles(files,family,'audioVaes',true)).toEqual(['wan_2.1_vae.safetensors']);
});
test('MOSS maps only matching companion identities and leaves renamed packed encoders to an explicit category override',()=>{
 const moss=families.find(f=>f.id==='moss-image')!;
 expect(moss).toMatchObject({multiple:16,clipType:'moss_image',pipelines:['t2i','i2i'],lora:[]});
 const files=[model('text_encoders','renamed.safetensors','qwen3.5-9b'),model('text_encoders','mossImage_sft_txt_int4.safetensors'),model('clip','qwen3.5_9b_wrong.safetensors','qwen3-vl-8b'),model('vae','renamed-vae.safetensors','flux2-vae'),model('vae','flux2-vae-wrong.safetensors','wan-2.1-vae')];
 expect(componentFiles(files,moss,'textEncoders')).toEqual(['renamed.safetensors']);
 expect(componentFiles(files,moss,'vaes')).toEqual(['renamed-vae.safetensors']);
 expect(componentFiles(files,moss,'textEncoders',true)).toContain('mossImage_sft_txt_int4.safetensors');
 expect(componentFiles(files,moss,'vaes',true)).not.toContain('mossImage_sft_txt_int4.safetensors');
});

test('Windows model subfolders do not satisfy filename hints and exact model identifiers survive selection',()=>{
 const files=[model('text_encoders','qwen_3_600m\\unrelated.gguf'),model('text_encoders','folder\\qwen_3_600m.gguf')];
 expect(componentFiles(files,family,'textEncoders')).toEqual(['folder\\qwen_3_600m.gguf']);
});

test('a truncated safetensors is never offered for automatic selection', () => {
  const files=[{...model('text_encoders','minimax_h3_text_encoder (1).safetensors'),integrity:'truncated'},model('text_encoders','minimax_h3_text_encoder.safetensors')];
  expect(componentFiles(files,{textEncoders:['minimax_h3']},'textEncoders')).toEqual(['minimax_h3_text_encoder.safetensors']);
});

test('component overrides default off and restored values require every boolean flag', () => {
 expect(Object.values(defaultComponentOverrides).every(value=>value===false)).toBe(true);
 expect(isComponentOverrides({...defaultComponentOverrides,textEncoders:true})).toBe(true);
 for(const value of [null,{},[],{...defaultComponentOverrides,textEncoders:'true'},{...defaultComponentOverrides,other:true}])expect(isComponentOverrides(value)).toBe(false);
});

test('auto-selection ranks by family pattern order: H3 defaults to the blending video VAE, not the windowed Sol decoder',()=>{
 const h3=families.find(f=>f.id==='minimax-h3')!;
 const files=[model('vae','MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors'),model('vae','MiniMaxH3/minimax_h3_sol_vae_fp16.safetensors'),model('vae','MiniMaxH3/minimax_h3_video_vae_int8_convrot.safetensors'),model('vae','MiniMaxH3/minimax_h3_video_vae_fp16.safetensors')];
 const vaes=componentFiles(files,h3,'vaes');
 expect(vaes[0]).toContain('minimax_h3_video_vae_fp16');
 expect(vaes[1]).toContain('minimax_h3_video_vae_int8');
 expect(vaes[2]).toContain('minimax_h3_sol_vae');
 expect(componentFiles(files,h3,'audioVaes')).toEqual(['MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors']);
});
