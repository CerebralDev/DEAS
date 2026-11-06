import {describe,it,expect} from 'vitest';
import families from '../families/builtin.json';
import {aceDurationStep,aceMusicProfile,defaultAceSettings,isAceSettings,normalizeAceDuration,aceLanguages,aceKeyScales} from './musicIntent';

describe('ACE-Step shared music contract',()=>{
 it.each([
  ['ace-step-1-0',50,5,5],['ace-step-1-5-base',50,7,1],['ace-step-1-5-sft',50,7,1],
  ['ace-step-1-5-turbo',8,1,3],['ace-step-1-5-xl-base',50,6,3],['ace-step-1-5-xl-sft',50,7,3],['ace-step-1-5-xl-turbo',8,1,3],
 ])('uses official music defaults without image geometry for %s',(family,steps,cfg,shift)=>{
  expect(aceMusicProfile(family as string)).toMatchObject({steps,cfg,shift,sampler:'euler',scheduler:'simple'});
  const f=families.find(f=>f.id===family)!;expect(f.pipelines).toEqual(['t2m']);expect(f).not.toHaveProperty('defaults');
 });
 it.each(['ace-step','ace-step-1-5','ace-step-1-5-xl','yue2',undefined])('does not resolve %s',(family)=>expect(aceMusicProfile(family)).toBeUndefined());
 it('validates every setting, enum, integer and finite range',()=>{
  expect(isAceSettings(defaultAceSettings)).toBe(true);
  for(const [k,v] of Object.entries(defaultAceSettings)){
   const missing={...defaultAceSettings} as Record<string,unknown>;delete missing[k];expect(isAceSettings(missing)).toBe(false);
   expect(isAceSettings({...defaultAceSettings,[k]:typeof v==='number'?NaN:null})).toBe(false);
  }
  for(const [k,v] of [['topP',1.01],['topK',.5],['bpm',301],['shift',Infinity],['keyScale','C'],['timeSignature','5'],['language','xx'],['unknown',true]] as const)expect(isAceSettings({...defaultAceSettings,[k]:v})).toBe(false);
  expect(aceLanguages).toHaveLength(51);expect(aceKeyScales).toHaveLength(34);
 });
 it('uses sample-derived duration grids and avoids every original truncation edge',()=>{
  expect(aceDurationStep('ace-step-1-0')).toBe(4096/44100);expect(aceDurationStep('ace-step-1-5-turbo')).toBe(.04);
  for(let frames=11;frames<=10766;frames++){
   const d=normalizeAceDuration(frames*4096/44100,'ace-step-1-0');
   expect(Math.floor(d*44100/512/8)).toBe(frames);
   expect(Math.abs(d-frames*4096/44100)).toBeLessThan(1e-10);
  }
  expect(normalizeAceDuration(120,'ace-step-1-0')).not.toBe(120);
  expect(normalizeAceDuration(120,'ace-step-1-5-turbo')).toBe(120);
 });
});
