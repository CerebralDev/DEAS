import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { I18nProvider, bundledCatalog } from '../../i18n/I18nProvider';
import { emptyLibrary, inFolder, matchesModel, modelKey } from '../../lib/modelLibrary';
import { LibraryEditor } from '../../components/models/ModelLibrary';
import type { LocalModel } from '../../lib/models';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:()=>true,convertFileSrc:(p:string)=>p}));
const m=(path:string):LocalModel=>({name:'same.safetensors',category:'loras',path,size:5,info:null,family:'minimax-h3'});
beforeEach(()=>{localStorage.clear();localStorage.setItem('deas.locale.v1','en');Object.defineProperty(HTMLDialogElement.prototype,'showModal',{configurable:true,value:function(){this.open=true;}});});
afterEach(()=>{cleanup();vi.resetAllMocks();});


it('stable source identities distinguish same filenames and search tags and IDs',()=>{
 const a=m('R:\\models\\same.safetensors'),b=m('S:\\models\\same.safetensors'),lib=emptyLibrary();lib.entries[modelKey(a)]={folder:'child',tags:['Band'],kind:'Motion'};lib.folders=[{id:'parent',name:'Music',parent:null},{id:'child',name:'People',parent:'parent'}];
 expect(modelKey(a)).not.toBe(modelKey(b));expect(matchesModel(a,lib,'band')).toBe(true);expect(matchesModel(b,lib,'band')).toBe(false);expect(inFolder(a,'parent',lib)).toBe(true);
});
it('folder removal reparents memberships, Undo restores and conflicts retain editable draft',async()=>{
 const initial=emptyLibrary(),a=m('R:\\models\\same.safetensors');initial.folders=[{id:'p',name:'Parent',parent:null},{id:'c',name:'Child',parent:'p'}];initial.entries[modelKey(a)]={folder:'c',tags:['band'],kind:'Motion'};
 const close=vi.fn();render(<I18nProvider><LibraryEditor models={[a]} initial={initial} onClose={close} onRefresh={vi.fn()}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Remove folder Child'}));fireEvent.click(screen.getByText('Undo'));expect(screen.getByRole('button',{name:'Remove folder Child'})).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Remove folder Child'}));vi.mocked(invoke).mockImplementation(async(c)=>{if(c==='native_model_library_write')throw new Error('changed elsewhere');return {id:'',state:'idle'};});
 fireEvent.click(screen.getByText('Save organization'));await screen.findByRole('alert');expect(localStorage.getItem('deas.library.unsaved')).toContain('band');const save=vi.mocked(invoke).mock.calls.find(([c])=>c==='native_model_library_write')![1] as {library:typeof initial};expect(save.library.entries[modelKey(a)].folder).toBe('p');expect(close).not.toHaveBeenCalled();
});
it('all new controls have dictionaries for every supported language',()=>{expect(bundledCatalog.diagnostics).toEqual([]);});
