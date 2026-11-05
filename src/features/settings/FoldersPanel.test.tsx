import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import { FoldersPanel, FolderFields } from './FoldersPanel';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:vi.fn(()=>true)}));
let bench:unknown={jobs:[]};
beforeEach(()=>{bench={jobs:[]};localStorage.clear();localStorage.setItem('deas.locale.v1','en');vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockImplementation(async(c,a)=>c==='native_settings_read'?{}:c==='native_job_current'?null:c==='native_workbench_current'?bench:c==='native_pick_directory'?'/new/models':c==='native_settings_move'?{folders:(a as {folders:unknown}).folders,movedFiles:2,warnings:[]}:(a as {folders:unknown}).folders);});
afterEach(()=>{cleanup();vi.resetAllMocks();});
const mount=()=>render(<I18nProvider><FoldersPanel root="/env"/></I18nProvider>);
async function choose(){await waitFor(()=>expect((screen.getByRole('button',{name:'Choose Models folder'}) as HTMLButtonElement).disabled).toBe(false));fireEvent.click(screen.getByRole('button',{name:'Choose Models folder'}));await screen.findByText('/new/models');}
it('keeps path-only saving separate from explicit movement',async()=>{
 mount();await choose();expect((screen.getByLabelText('Move existing files to the selected folders') as HTMLInputElement).checked).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'Save folders'}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_settings_write',{root:'/env',folders:{models:'/new/models'}}));
 expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_settings_move')).toBe(false);
});
it('requires an explicit move selection and reports only native results',async()=>{
 mount();await choose();fireEvent.click(screen.getByLabelText('Move existing files to the selected folders'));fireEvent.click(screen.getByRole('button',{name:'Move files & save folders'}));
 await screen.findByText('2 files moved');expect(invoke).toHaveBeenCalledWith('native_settings_move',{root:'/env',folders:{models:'/new/models'},confirmed:true});
 expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_settings_write')).toBe(false);
});
it('adds extra model roots to the payload only when chosen and drops them on reset',async()=>{
 mount();await choose();
 const help=screen.getByRole('button',{name:'Choose Models folder'}).closest('[title]')?.getAttribute('title');
 expect(help).toContain('Stable-Diffusion');expect(help).toContain('Lora');
 fireEvent.click(screen.getByRole('button',{name:'Add another model folder'}));
 expect(screen.getByRole('button',{name:'Choose additional model folder 1'}).closest('[title]')?.getAttribute('title')).toBe(help);
 await screen.findByText('Additional model folder 1');
 fireEvent.click(screen.getByRole('button',{name:'Save folders'}));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_settings_write',{root:'/env',folders:{models:'/new/models'}}));
 fireEvent.click(screen.getByRole('button',{name:'Add another model folder'}));
 fireEvent.click(screen.getByRole('button',{name:'Choose additional model folder 1'}));
 await waitFor(()=>{fireEvent.click(screen.getByRole('button',{name:'Save folders'}));expect(invoke).toHaveBeenLastCalledWith('native_settings_write',{root:'/env',folders:{models:'/new/models',extraModels:['/new/models']}});});
 fireEvent.click(screen.getByRole('button',{name:'Remove additional model folder 1'}));
 await waitFor(()=>{fireEvent.click(screen.getByRole('button',{name:'Save folders'}));expect(invoke).toHaveBeenLastCalledWith('native_settings_write',{root:'/env',folders:{models:'/new/models'}});});
});
it('keeps failed moves editable without claiming success',async()=>{
 mount();await choose();vi.mocked(invoke).mockRejectedValueOnce('Filename collision: existing.safetensors');fireEvent.click(screen.getByLabelText('Move existing files to the selected folders'));fireEvent.click(screen.getByRole('button',{name:'Move files & save folders'}));
 await screen.findByRole('alert');expect(screen.queryByText('2 files moved')).toBeNull();expect(screen.getByText('/new/models')).toBeTruthy();
});
it('restarts a running backend after saving, but never over a running generation',async()=>{
 const onRestart=vi.fn();render(<I18nProvider><FoldersPanel root="/env" onRestart={onRestart}/></I18nProvider>);await choose();
 fireEvent.click(screen.getByRole('button',{name:'Save folders'}));
 await screen.findByText(/^Saved\. A running backend restarts with the new folders/);expect(onRestart).toHaveBeenCalledOnce();
 bench={jobs:[{id:'w',state:'running'}]};fireEvent.click(screen.getByRole('button',{name:'Reset Models folder'}));
 fireEvent.click(screen.getByRole('button',{name:'Save folders'}));await screen.findByText(/A job is running/);expect(onRestart).toHaveBeenCalledOnce();
});
it('draft folders update installation intent without any settings read or write',async()=>{
 const changed=vi.fn();render(<I18nProvider><FoldersPanel root="/not-created-yet" draft={{}} onChange={changed}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Choose Models folder'}));await waitFor(()=>expect(changed).toHaveBeenCalledWith({models:'/new/models'}));expect(vi.mocked(invoke).mock.calls.map(([c])=>c)).toEqual(['native_pick_directory']);expect(screen.queryByLabelText('Move existing files to the selected folders')).toBeNull();
});
it('ignores a chooser result after unmount',async()=>{
 let resolve!:(v:string)=>void;vi.mocked(invoke).mockImplementationOnce(()=>new Promise(r=>{resolve=r;}) as never);const changed=vi.fn();const view=render(<I18nProvider><FolderFields folders={{}} onChange={changed}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Choose Models folder'}));view.unmount();await act(async()=>resolve('/late'));expect(changed).not.toHaveBeenCalled();
});
