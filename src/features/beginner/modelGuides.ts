import animaImage from './assets/anima.png';
import mossImage from './assets/moss.webp';
import kreaImage from './assets/krea2.png';
import illustriousImage from './assets/illustrious.png';
import type {LocalModel} from '../../lib/models';

/** Beginner editorial metadata only. Never used to infer recipe compatibility. */
export interface ModelGuide {
 id:string;name:string;family:string;tags:string[];aliases:string[];
 summary:string;prompting:string;note:string;image:string;credit:string;source:string;
 repo:string;files:string[];target:'diffusion_models'|'checkpoints';
}
export const MODEL_GUIDES:readonly ModelGuide[]=[
 {id:'anima-base',name:'Anima Base v1.0',family:'anima',
  tags:['anime','illustration','characters','painting','fantasy'],aliases:['drawing','manga','dessin','personnages','peinture'],
  summary:'An art-focused base model with a broad range of anime characters and illustration styles. A flexible starting point when you want to guide the look yourself.',
  prompting:'Use descriptive English, lowercase Danbooru-style tags, or both. Describe the subject, appearance, pose and setting. For prose, give it a couple of clear sentences rather than a few vague words.',
  note:'Best for drawn and artistic images; photorealism is not its strength. This is Base, not Aesthetic or Turbo.',
  image:animaImage,credit:'CircleStone Labs / Comfy Org · family illustration from the preview checkpoint',source:'https://huggingface.co/circlestone-labs/Anima',
  repo:'circlestone-labs/Anima',files:['anima-base-v1.0.safetensors'],target:'diffusion_models'},
 {id:'moss-sft',name:'MOSS-Image SFT',family:'moss-image',
  tags:['anime','photo','illustration','text','design','layouts'],aliases:['photography','photorealistic','realistic','realism','réaliste','realiste','typography','poster','affiche','manga','drawing'],
  summary:'A versatile image model for photographs, artwork and compositions with written text. It can work with natural descriptions, tags and structured layouts.',
  prompting:'Describe the visible scene, then specify the medium, lighting and composition. For text, give the exact wording and where it should appear; line breaks help with longer passages. Ordinary prose is enough to start.',
  note:'The refined SFT checkpoint is the featured MOSS version here. Structured JSON layouts are optional; no need to start there.',
  image:mossImage,credit:'OpenMOSS · SFT side of the official Base/SFT comparison',source:'https://huggingface.co/OpenMOSS-Team/MOSS-Image-SFT',
  repo:'Comfy-Org/MOSS-Image',files:['moss_image_sft_bf16.safetensors','moss_image_sft_int8_convrot.safetensors'],target:'diffusion_models'},
 {id:'krea2-raw',name:'Krea 2 Raw',family:'krea-2',
  tags:['anime','photo','illustration','design','3D','painting'],aliases:['krea2','photography','photorealistic','realistic','realism','réaliste','realiste','manga','drawing','peinture','render'],
  summary:'A broad creative base model for exploring photographic and stylized images. Raw is the undistilled release, with room for different visual directions.',
  prompting:'Write as if describing an image to another person. Name the subject, setting, medium, framing, light and mood. Use specific visual details; put exact words in quotation marks when asking for lettering.',
  note:'Ask explicitly for the style you want. This is Raw, not the faster distilled Turbo version.',
  image:kreaImage,credit:'Krea · official Raw example 00',source:'https://huggingface.co/krea/Krea-2-Raw',
  repo:'Comfy-Org/Krea-2',files:['krea2_raw_bf16.safetensors','krea2_raw_fp8_scaled.safetensors','krea2_raw_int8_convrot.safetensors'],target:'diffusion_models'},
 {id:'illustrious-base',name:'Illustrious XL v0.1 Base',family:'sdxl',
  tags:['anime','illustration','characters','fantasy'],aliases:['illustrious','manga','drawing','dessin','personnages'],
  summary:'The original illustration-focused Illustrious base checkpoint, built on SDXL. It brings a large vocabulary of characters, artistic styles and visual concepts from Danbooru.',
  prompting:'Start with concise, comma-separated tags: subject, appearance, outfit, pose and background. Be explicit about the composition and art style. Use the model’s own guidance rather than copying a derivative’s trigger words.',
  note:'This is the untuned v0.1 base, not GUIDED, a community merge or an aesthetic fine-tune.',
  image:illustriousImage,credit:'OnomaAI Research · official v0.1 model-card sample s00',source:'https://huggingface.co/OnomaAIResearch/Illustrious-xl-early-release-v0',
  repo:'OnomaAIResearch/Illustrious-xl-early-release-v0',files:['Illustrious-XL-v0.1.safetensors'],target:'checkpoints'},
];
const stem=(name:string)=>(name.split(/[\\/]/).pop()??name).replace(/\.(safetensors|sft|ckpt|gguf|pt|pth)$/i,'').toLowerCase().replace(/[ _-]/g,'');
/** An exact known base filename plus the declared family; never a substring guess. */
export const baseGuide=(m:LocalModel)=>MODEL_GUIDES.find(g=>m.family===g.family&&g.files.some(f=>stem(f)===stem(m.name)));
/** Family tips are labelled as such; SDXL alone does not identify Illustrious. */
export const installedGuide=(m:LocalModel)=>baseGuide(m)??MODEL_GUIDES.find(g=>g.family!=='sdxl'&&g.family===m.family);
export function matchesGuide(query:string,text:string,guide?:ModelGuide){
 const normalized=(v:string)=>v.normalize('NFKD').replace(/\p{M}/gu,'').toLocaleLowerCase();
 const haystack=normalized([text,guide?.name,...(guide?.tags??[]),...(guide?.aliases??[])].join(' '));
 return normalized(query).trim().split(/\s+/).every(word=>haystack.includes(word));
}
