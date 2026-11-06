import {afterEach,beforeEach,expect,test,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import {invoke} from '@tauri-apps/api/core';
import {I18nProvider} from '../../i18n/I18nProvider';
import {readDownloadCatalog} from '../../lib/downloads';
import {BeginnerModels} from './BeginnerModels';
import {ExperienceWelcome,ExperienceSwitch} from './Experience';
import {filterInstalled,installedMedia,catalogModels} from './modelCatalog';
import {MODEL_GUIDES,baseGuide,matchesGuide} from './modelGuides';
import type {LocalModel} from '../../lib/models';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(async()=>null),isTauri:()=>true}));
vi.mock('../../lib/downloads',async original=>({...await original<typeof import('../../lib/downloads')>(),readDownloadCatalog:vi.fn()}));
vi.mock('../downloads/DownloadPluginPanel',()=>({DownloadPanel:({initialEntryId}:{initialEntryId:string})=><div data-real-download-entry={initialEntryId}/> }));
const models:LocalModel[]=[{name:'Krea.safetensors',category:'diffusion_models',path:'/m/krea',size:10,info:null,family:'krea-2'},{name:'MOSS.safetensors',category:'diffusion_models',path:'/m/moss',size:10,info:null,family:'moss-image'},{name:'video.safetensors',category:'diffusion_models',path:'/m/video',size:10,info:null,family:'minimax-h3'},{name:'unknown.safetensors',category:'checkpoints',path:'/m/unknown',size:10,info:null}];
const entries=[{id:'moss',label:'MOSS',providerId:'hf',scope:'repo' as const,locator:'provider/model',kind:'model',target:'diffusion_models',media:'image' as const},{id:'video',label:'Video',providerId:'hf',scope:'repo' as const,locator:'provider/video',kind:'model',target:'diffusion_models',media:'video' as const}];
beforeEach(()=>{localStorage.clear();localStorage.setItem('deas.locale.v1','en');vi.mocked(readDownloadCatalog).mockResolvedValue({version:1,providers:[],entries});});
afterEach(()=>{cleanup();vi.clearAllMocks();});
test('image/video uses declared capabilities, with no style categories or filename guesses',()=>{
 expect(filterInstalled(models,'image').map(m=>m.family)).toEqual(['krea-2','moss-image']);
 expect(installedMedia(models[3])).toBeUndefined();expect(filterInstalled(models,'all')).toHaveLength(4);
 expect(catalogModels(entries,'video').map(e=>e.id)).toEqual(['video']);
});
test('onboarding choices and switch report the chosen experience',()=>{
 const choose=vi.fn();render(<I18nProvider><ExperienceWelcome onChoose={choose}/><ExperienceSwitch value="beginner" onChange={choose}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:/A beginner/}));expect(choose).toHaveBeenCalledWith('beginner');
 fireEvent.click(screen.getByRole('switch',{name:'Advanced'}));expect(choose).toHaveBeenLastCalledWith('advanced');
});
test('installed selection uses exact model identity; Explorer hands off to the existing download panel only on click',async()=>{
 const use=vi.fn();render(<I18nProvider><BeginnerModels models={models} root="/env" onUse={use} onRefresh={()=>{}} reason={()=>''} loading={false}/></I18nProvider>);
 expect(readDownloadCatalog).not.toHaveBeenCalled();fireEvent.click(screen.getAllByRole('button',{name:/Use this model/})[0]);expect(use).toHaveBeenCalledWith(models[0]);
 fireEvent.click(screen.getByRole('tab',{name:'Explore'}));await waitFor(()=>expect(screen.getAllByRole('button',{name:/Download this model/})).toHaveLength(6));
 expect(document.querySelector('[data-real-download-entry]')).toBeNull();
 fireEvent.click(within(screen.getByRole('heading',{name:'MOSS'}).closest('article')!).getByRole('button',{name:/Download this model/}));expect(document.querySelector('[data-real-download-entry]')?.getAttribute('data-real-download-entry')).toBe('moss');
 expect(invoke).not.toHaveBeenCalled();
});

test('style tags are non-exclusive and search is accent-insensitive with multiple terms',()=>{
 expect(MODEL_GUIDES.filter(g=>matchesGuide('anime',g.name,g)).map(g=>g.id)).toEqual(['anima-base','moss-sft','krea2-raw','illustrious-base']);
 expect(MODEL_GUIDES.filter(g=>matchesGuide('anime photo',g.name,g)).map(g=>g.id)).toEqual(['moss-sft','krea2-raw']);
 expect(MODEL_GUIDES.filter(g=>matchesGuide('réaliste',g.name,g)).map(g=>g.id)).toEqual(['moss-sft','krea2-raw']);
 expect(baseGuide({...models[1],name:'moss_image_base_bf16.safetensors'})).toBeUndefined();
 expect(baseGuide({...models[1],name:'moss_image_sft_bf16.safetensors'})?.id).toBe('moss-sft');
 expect(baseGuide({...models[0],name:'custom_krea2_raw_bf16.safetensors'})).toBeUndefined();
 expect(baseGuide({...models[0],name:'krea2_turbo_bf16.safetensors'})).toBeUndefined();
 expect(baseGuide({...models[3],name:'Illustrious-XL-v0.1.safetensors'})).toBeUndefined();
});

test('Explore includes four illustrated English guides and clickable tags; Installed never invents files',async()=>{
 render(<I18nProvider><BeginnerModels models={models} onUse={()=>{}} onRefresh={()=>{}} reason={()=>''} loading={false}/></I18nProvider>);
 expect(document.querySelector('[data-featured-models]')).toBeNull();
 fireEvent.click(screen.getByRole('tab',{name:'Explore'}));
 expect(document.querySelectorAll('[data-guide]')).toHaveLength(4);
 expect(screen.getByRole('img',{name:'MOSS-Image SFT'})).toBeTruthy();
 expect(screen.queryByRole('heading',{name:'MOSS-Image Base'})).toBeNull();
 const card=document.querySelector('[data-guide="krea2-raw"]')!;
 expect(within(card as HTMLElement).getByText('How to prompt')).toBeTruthy();
 fireEvent.click(within(card as HTMLElement).getByRole('button',{name:'Search tag: photo'}));
 expect(document.querySelectorAll('[data-guide]')).toHaveLength(2);
 expect(document.querySelector('[data-guide="anima-base"]')).toBeNull();
 await waitFor(()=>expect(readDownloadCatalog).toHaveBeenCalled());
 expect(invoke).not.toHaveBeenCalled();
});
test('catalog failure is visible and forwarded to the central native log without error payloads',async()=>{
 vi.mocked(readDownloadCatalog).mockRejectedValue(new Error('private token must not cross logging boundary'));
 render(<I18nProvider><BeginnerModels models={[]} onUse={()=>{}} onRefresh={()=>{}} reason={()=>''} loading={false}/></I18nProvider>);
 fireEvent.click(screen.getByRole('tab',{name:'Explore'}));await waitFor(()=>expect(screen.getByRole('alert').textContent).toContain('could not be loaded'));
 expect(invoke).toHaveBeenCalledWith('native_logs_beginner_error',{operation:'catalog'});
 expect(screen.queryByText(/private token/)).toBeNull();
});
