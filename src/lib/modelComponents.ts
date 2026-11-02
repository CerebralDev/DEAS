import type { LocalModel } from './models';

export const componentSlots=['textEncoders','vaes','audioVaes','textProjections'] as const;
export type ComponentSlot=typeof componentSlots[number];
export interface ComponentFamily {
 textEncoders?:string[]; vaes?:string[]; audioVaes?:string[]; textProjections?:string[];
 components?:Partial<Record<ComponentSlot,string[]>>;
}
export const defaultComponentOverrides={textEncoders:false,vaes:false,audioVaes:false,textProjections:false};
export const isComponentOverrides=(value:unknown):value is typeof defaultComponentOverrides=>!!value&&typeof value==='object'&&Object.keys(value).length===componentSlots.length&&componentSlots.every(key=>typeof (value as Record<string,unknown>)[key]==='boolean');

/** Header identities win over filename hints; explicit overrides only widen the current category. Broken files are never offered, so auto-selection cannot pick a truncated model. */
export function componentFiles(models:readonly LocalModel[],family:ComponentFamily,slot:ComponentSlot,all=false):string[] {
 const categories=slot==='vaes'||slot==='audioVaes'?['vae']:['text_encoders','clip'];
 return [...new Set(models.filter(model=>categories.includes(model.category)&&!model.integrity&&(slot==='textEncoders'||!/\.gguf$/i.test(model.name))&&(all||(model.component
  ?family.components?.[slot]?.includes(model.component)
  :(family[slot]??[]).some(pattern=>(model.name.split(/[\\/]/).pop()??'').toLowerCase().includes(pattern)))))
  .map(model=>model.name))].sort((a,b)=>a.localeCompare(b));
}

/** Second encoder of a dual-loader family: a filename hint only, never a header-compatibility claim. */
export function componentFilesNamed(models:readonly LocalModel[],patterns:readonly string[],all=false):string[] {
 const wanted=(model:LocalModel)=>patterns.some(pattern=>(model.name.split(/[\\/]/).pop()??'').toLowerCase().includes(pattern));
 return [...new Set(models.filter(model=>['text_encoders','clip'].includes(model.category)&&!model.integrity&&!/\.gguf$/i.test(model.name)&&(all||wanted(model))).map(model=>model.name))].sort((a,b)=>a.localeCompare(b));
}
