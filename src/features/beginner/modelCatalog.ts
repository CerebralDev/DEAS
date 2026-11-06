import families from '../../families/builtin.json' with {type:'json'};
import type {LocalModel} from '../../lib/models';
import type {CatalogEntry} from '../../lib/downloads';
export type ModelMedia='all'|'image'|'video';
export const modelFamily=(model:LocalModel)=>families.find(f=>f.id===model.family);
/** Classify only declared capabilities. Unknown architectures stay in All. */
export function installedMedia(model:LocalModel):Exclude<ModelMedia,'all'>|undefined {
 const pipelines=modelFamily(model)?.pipelines??[];
 if(pipelines.some(p=>['t2v','i2v','v2v','ref2v'].includes(p)))return 'video';
 if(pipelines.some(p=>['t2i','i2i'].includes(p)))return 'image';
 return undefined;
}
/** These are model packs, not a style taxonomy or a filename-based compatibility guess. */
export const catalogModels=(entries:CatalogEntry[],media:ModelMedia)=>entries.filter(e=>['checkpoints','diffusion_models','unet'].includes(e.target)&&(media==='all'||e.media===media));
export const filterInstalled=(models:LocalModel[],media:ModelMedia)=>models.filter(m=>media==='all'||installedMedia(m)===media);
