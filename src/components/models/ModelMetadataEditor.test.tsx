import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { writeModelFamily, type LocalModel } from '../../lib/models';
import { I18nProvider } from '../../i18n/I18nProvider';
import { ModelMetadataTarget, useModelMetadataUpdates } from './ModelMetadataEditor';
import { useState } from 'react';
import families from '../../families/builtin.json' with { type: 'json' };
import { bundledCatalog } from '../../i18n/I18nProvider';
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true}));
vi.mock('../../lib/models',()=>({writeModelFamily:vi.fn()}));
const model:LocalModel={name:'nested/model.safetensors',path:'/models/nested/model.safetensors',category:'diffusion_models',size:1,info:null};
beforeEach(()=>{localStorage.setItem('deas.locale.v1','en');Object.defineProperty(HTMLDialogElement.prototype,'showModal',{configurable:true,value:function(){this.open=true;}});});
afterEach(()=>{cleanup();vi.resetAllMocks();});
const mount=(m=model)=>render(<I18nProvider><ModelMetadataTarget model={m}><button>{m.name}</button></ModelMetadataTarget></I18nProvider>);
function open(){fireEvent.contextMenu(screen.getByRole('button',{name:model.name}));fireEvent.click(screen.getByRole('menuitem',{name:'Edit meta…'}));}
test('cancel is local and save writes only the declared family',async()=>{
 mount();open();expect(screen.getByRole('dialog').textContent).toContain(model.path);expect(screen.getByText('Unknown',{selector:'dd'})).toBeTruthy();fireEvent.click(screen.getByRole('button',{name:'Cancel'}));expect(writeModelFamily).not.toHaveBeenCalled();
 open();fireEvent.change(screen.getByLabelText('Model family'),{target:{value:'anima'}});vi.mocked(writeModelFamily).mockResolvedValue({...model,family:'anima',familyOverride:'anima'});fireEvent.click(screen.getByRole('button',{name:'Save'}));await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());expect(writeModelFamily).toHaveBeenCalledExactlyOnceWith(model.path,'anima');
});
test('detection stays distinct from the declaration and reset writes null',async()=>{
 mount({...model,detectedFamily:'anima',family:'krea-2',familyOverride:'krea-2'});open();
 const entries=[...screen.getByRole('dialog').querySelectorAll('dd')].map(n=>n.textContent);
 expect(entries).toContain('Anima');expect(entries).toContain('Krea 2');expect(screen.getByLabelText('Model family')).toHaveProperty('value','krea-2');
 expect([...screen.getByLabelText('Model family').querySelectorAll('option')].map(o=>o.value)).toEqual(['',...families.filter(f=>['checkpoint','diffusion'].includes(f.loader)).map(f=>f.id)]);
 vi.mocked(writeModelFamily).mockResolvedValue({...model,detectedFamily:'anima',family:'anima'});
 fireEvent.click(screen.getByRole('button',{name:'Reset to detection'}));await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());expect(writeModelFamily).toHaveBeenCalledExactlyOnceWith(model.path,null);
});
test('LoRA declarations offer only native-valid recipe families with LoRA signatures',()=>{
 mount({...model,category:'loras',detectedFamily:'sdxl'});open();
 const ids=[...screen.getByLabelText('Model family').querySelectorAll('option')].map(o=>o.value);
 expect(ids).toEqual(['',...families.filter(f=>['checkpoint','diffusion'].includes(f.loader)&&f.lora.length>0).map(f=>f.id)]);
 expect(ids).not.toContain('moss-image');expect(ids).not.toContain('sam3');
});
test('failed saves stay open, report error and allow retry without duplicate busy writes',async()=>{
 mount({...model,detectedFamily:'anima'});open();expect(screen.getByLabelText('Model family')).toHaveProperty('value','anima');
 let fail!:(error:Error)=>void;vi.mocked(writeModelFamily).mockImplementation(()=>new Promise((_,reject)=>{fail=reject;}));
 fireEvent.click(screen.getByRole('button',{name:'Save'}));
 for(const name of ['Save','Reset to detection','Cancel'])expect(screen.getByRole('button',{name})).toHaveProperty('disabled',true);
 fireEvent.click(screen.getByRole('button',{name:'Save'}));expect(writeModelFamily).toHaveBeenCalledTimes(1);
 fireEvent(screen.getByRole('dialog'),new Event('cancel',{cancelable:true}));expect(screen.getByRole('dialog')).toBeTruthy();
 await act(async()=>fail(new Error('disk full')));expect(screen.getByRole('alert').textContent).toBe('Could not save model metadata. Try again.');expect(screen.getByRole('button',{name:'Save'})).toHaveProperty('disabled',false);
 vi.mocked(writeModelFamily).mockResolvedValue({...model,family:'anima',familyOverride:'anima'});fireEvent.click(screen.getByRole('button',{name:'Save'}));await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());
});
test('late unmounted and replaced-target writes cannot publish a model update',async()=>{
 let finish!:(value:LocalModel)=>void;vi.mocked(writeModelFamily).mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
 const changed=vi.fn();window.addEventListener('deas:model-family-changed',changed);
 const view=mount({...model,detectedFamily:'anima'});open();fireEvent.click(screen.getByRole('button',{name:'Save'}));
 const next={...model,name:'replacement.safetensors',path:'/else/replacement.safetensors'};
 view.rerender(<I18nProvider><ModelMetadataTarget model={next}><button>{next.name}</button></ModelMetadataTarget></I18nProvider>);
 await act(async()=>finish({...model,family:'anima'}));expect(changed).not.toHaveBeenCalled();expect(screen.getByRole('dialog').textContent).toContain(next.path);
 view.unmount();mount({...model,detectedFamily:'anima'});open();fireEvent.click(screen.getByRole('button',{name:'Save'}));cleanup();await act(async()=>finish({...model,family:'anima'}));expect(changed).not.toHaveBeenCalled();window.removeEventListener('deas:model-family-changed',changed);
});
test('mounted observers update only exact paths and reset removes the declaration',async()=>{
 function Observer(){const [models,setModels]=useState<LocalModel[]|null>([{...model,detectedFamily:'anima',family:'krea-2',familyOverride:'krea-2'},{...model,path:'/other/same-name.safetensors'}]);useModelMetadataUpdates(setModels);return <output data-testid="models">{JSON.stringify(models)}</output>;}
 render(<I18nProvider><Observer/><ModelMetadataTarget model={{...model,detectedFamily:'anima',family:'krea-2',familyOverride:'krea-2'}}><button>{model.name}</button></ModelMetadataTarget></I18nProvider>);open();
 vi.mocked(writeModelFamily).mockResolvedValue({...model,detectedFamily:'anima',family:'anima'});fireEvent.click(screen.getByRole('button',{name:'Reset to detection'}));await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());
 const updated=JSON.parse(screen.getByTestId('models').textContent!);expect(updated[0]).toEqual({...model,detectedFamily:'anima',family:'anima'});expect(updated[1]).not.toHaveProperty('family');
});
test('context-menu key opens focused compact action and Escape restores tile focus',()=>{
 mount();const button=screen.getByRole('button',{name:model.name});fireEvent.keyDown(button,{key:'ContextMenu'});expect(document.activeElement).toBe(screen.getByRole('menuitem'));fireEvent.keyDown(screen.getByRole('menuitem'),{key:'Escape'});expect(screen.queryByRole('menu')).toBeNull();expect(document.activeElement).toBe(button);expect(writeModelFamily).not.toHaveBeenCalled();
});
test.each([
 ['header','Identified from header metadata or tensor structure.'],
 ['filename','Variant inferred from filename; not confirmed by the header.'],
 ['unresolved','Variant not identified. Declare the exact model family before using model-specific defaults.'],
 ['conflict','Conflicting model identity evidence. Check the file and declare its exact family.'],
] as const)('ACE-Step detection explains %s evidence without changing the declaration', (familyEvidence,text)=>{
 mount({...model,detectedFamily:'ace-step',family:'ace-step',familyEvidence});open();
 expect(screen.getByText(text)).toBeTruthy();
 expect(document.querySelector('[data-model-family-evidence]')?.getAttribute('data-model-family-evidence')).toBe(familyEvidence);
 expect(writeModelFamily).not.toHaveBeenCalled();
});
test('ACE-Step variants use the existing family declaration and keep automatic evidence separate',async()=>{
 const ace={...model,detectedFamily:'ace-step-1-5',family:'ace-step-1-5',familyEvidence:'unresolved' as const};
 mount(ace);open();
 const select=screen.getByLabelText('Model family');
 for(const entry of families.filter(f=>f.id.startsWith('ace-step'))){
  expect([...select.querySelectorAll('option')].some(o=>o.value===entry.id)).toBe(true);
  expect(entry.pipelines).toEqual('music' in entry?['t2m']:[]);
 }
 fireEvent.change(select,{target:{value:'ace-step-1-5-xl-sft'}});
 vi.mocked(writeModelFamily).mockResolvedValue({...ace,family:'ace-step-1-5-xl-sft',familyOverride:'ace-step-1-5-xl-sft'});
 fireEvent.click(screen.getByRole('button',{name:'Save'}));
 await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());
 expect(writeModelFamily).toHaveBeenCalledExactlyOnceWith(model.path,'ace-step-1-5-xl-sft');
});

test('models without ACE-Step evidence keep the existing metadata surface',()=>{
 mount({...model,detectedFamily:'anima'});open();
 expect(document.querySelector('[data-model-family-evidence]')).toBeNull();
});

test('all metadata strings have six-locale data-only parity',()=>{
 const dictionaries=import.meta.glob('./locales/*.json',{eager:true,import:'default'}) as Record<string,Record<string,string>>;
 const entries=Object.values(dictionaries);expect(entries).toHaveLength(6);for(const dict of entries){expect(Object.keys(dict).sort()).toEqual(Object.keys(entries[0]).sort());expect(Object.values(dict).every(v=>typeof v==='string'&&v.length>0)).toBe(true);}expect(bundledCatalog.diagnostics).toEqual([]);
});
