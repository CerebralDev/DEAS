import { readAuthStatus } from '../../lib/downloads';
import { StrictMode } from 'react';
import { cleanup, fireEvent, render, screen, waitFor, act } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import AiOSetup, { nextFolder } from './AiOSetup';
import { defaultLlmConfig } from '../../lib/llm';
import { I18nProvider } from '../../i18n/I18nProvider';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { NativeInstallRequest } from '../../lib/setup';
// Backend embeds auth now; lifecycle fixtures provide its real empty metadata schema.
vi.mock('../../lib/downloads',async original=>({...await original<typeof import('../../lib/downloads')>(),readAuthStatus:vi.fn(async()=>[])}));
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:vi.fn(()=>false)}));
// File existence probe: only takeover fixture paths (…/ComfyUI/main.py) report as present; everything else stays "not local".
vi.mock('@tauri-apps/plugin-fs',()=>({exists:vi.fn(async(p:string)=>String(p).endsWith('ComfyUI/main.py'))}));
beforeEach(()=>{vi.mocked(readAuthStatus).mockResolvedValue([]);localStorage.clear();localStorage.setItem('deas.pref.backend.autostart','false');localStorage.setItem('deas.locale.v1','en');vi.mocked(isTauri).mockReturnValue(false);});
afterEach(()=>{cleanup();vi.useRealTimers();vi.resetAllMocks();});
const click=(name:string)=>fireEvent.click(screen.getByRole(name==='Backend'||name==='Logs'?'tab':'button',{name}));
const mount=(connect=vi.fn(),installed=vi.fn())=>render(<I18nProvider><AiOSetup onConnectExisting={connect} onInstalled={installed}/></I18nProvider>);
const open=()=>{fireEvent.click(screen.getByRole('tab',{name:'Backend'}));};
const change=(label:string,value:string)=>fireEvent.change(screen.getByLabelText(label),{target:{value}});
const request:NativeInstallRequest={storagePath:'/app/deas-environment',backend:'cpu',features:['director','forge','refmods','continuity'],options:{sage:false,radial:false,flash:false,ffmpeg:false},comfyRef:'master'};
const defaults={storagePath:request.storagePath,backend:'cpu',hardware:null,warnings:['CPU fallback fixture']};
const plan=(r=request)=>({id:'fixture-plan',request:r,steps:[],warnings:[],unsupportedOptions:[],canExecute:true});
const DEPS=[{id:'environment',ok:true},{id:'python',ok:true},{id:'comfyui',ok:true},{id:'nodes',ok:true},{id:'api',ok:true}];
function nativeFixture(){
 vi.mocked(isTauri).mockReturnValue(true);
 vi.mocked(invoke).mockImplementation(async(command,args)=>{if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;if(command==='native_llm_models')return {models:[],warnings:[]};
  if(command==='native_install_defaults')return defaults;
  if(command==='native_install_plan')return plan((args as {request:NativeInstallRequest}).request);
  if(command==='native_detect_hardware')return {backend:null};
  if(command==='native_backend_test')return DEPS;
  return {state:'running',stepId:'python',logs:['native fixture log'],error:null};
 });
}
async function ready(){mount();open();await waitFor(()=>expect((screen.getByLabelText('Storage directory') as HTMLInputElement).value).toBe(request.storagePath));await screen.findByRole('button',{name:'Start install'});}
function deferred<T>(){let resolve!:(v:T)=>void;const promise=new Promise<T>(r=>{resolve=r;});return {promise,resolve};}
describe('automatic-first installation — read-only mock IPC fixtures',()=>{
 it('retains the backend authorization draft across main-tab navigation',async()=>{
  nativeFixture();mount();const token=await screen.findByLabelText('Token');
  fireEvent.change(token,{target:{value:'draft-only-not-a-credential'}});
  fireEvent.click(screen.getByRole('tab',{name:'Studio'}));expect(token.isConnected).toBe(true);expect(token.closest('[hidden]')).not.toBeNull();
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));expect((screen.getByLabelText('Token') as HTMLInputElement).value).toBe('draft-only-not-a-credential');
  expect(vi.mocked(invoke).mock.calls.some(([c])=>['native_auth_set','native_auth_remove','native_auth_restore'].includes(c))).toBe(false);
 });
 it('uses a left menu and one right pane for startup, LLM and authorization',()=>{
  mount();
  const menu=screen.getByRole('tablist',{name:'Backend panels'});
  expect(menu.getAttribute('aria-orientation')).toBe('vertical');
  expect(menu.querySelectorAll('[role="tab"]')).toHaveLength(3);
  expect(screen.getByRole('tab',{name:'Startup optimizations'}).hasAttribute('disabled')).toBe(true);
  expect(screen.getByRole('tab',{name:'LLM backend'}).hasAttribute('disabled')).toBe(false);
  expect(screen.getByRole('tab',{name:'Provider authorization'}).getAttribute('aria-selected')).toBe('true');
  expect(screen.getAllByRole('tabpanel')).toHaveLength(1);
 });
 it('keeps the footer start hint until running, then briefly confirms without repeating on status reads',async()=>{
  nativeFixture();let backend='starting';
  vi.mocked(invoke).mockImplementation(async(command,args)=>{
   if(command==='native_install_defaults')return defaults;
   if(command==='native_install_plan')return plan((args as {request:NativeInstallRequest}).request);
   if(command==='native_install_start')return {state:'succeeded',logs:[],error:null};
   if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;
   if(command==='native_llm_models')return {models:[],warnings:[]};
   if(command==='native_settings_read')return {};
   if(command==='native_backend_test')return DEPS;
   if(command==='native_backend_status')return {state:backend,endpoint:backend==='running'?'http://127.0.0.1:49312':null,logs:[],error:null,generationReady:false};
   throw Error('Unexpected fixture IPC '+command);
  });
  await ready();click('Start install');await screen.findByRole('tab',{name:'Studio',selected:true});click('Backend');click('Test');
  await screen.findByText('Backend starting…');vi.useFakeTimers();
  await act(()=>vi.advanceTimersByTimeAsync(10000));expect(screen.getByText('Backend starting…')).toBeTruthy();
  backend='running';await act(async()=>{click('Test');});
  expect(document.querySelector('[data-backend-notice]')?.textContent).toContain('✓');
  expect(screen.getByText('Backend started')).toBeTruthy();
  await act(()=>vi.advanceTimersByTimeAsync(2000));
  await act(async()=>{click('Test');});
  await act(()=>vi.advanceTimersByTimeAsync(1001));expect(document.querySelector('[data-backend-notice]')).toBeNull();
  backend='starting';await act(async()=>{click('Test');});expect(screen.getByText('Backend starting…')).toBeTruthy();
  backend='failed';await act(async()=>{click('Test');});expect(document.querySelector('[data-backend-notice]')).toBeNull();
 });
 it('shows configured-provider marks in the overview and refreshes them with inline authorization',async()=>{
  nativeFixture();vi.mocked(readAuthStatus).mockResolvedValue([{providerId:'github',configured:true,persisted:true}]);
  mount();
  await waitFor(()=>expect(document.querySelector('[data-provider-configured="github"]')).not.toBeNull());
  expect(document.querySelector('[data-provider-configured="huggingface"]')).toBeNull();
  expect(document.querySelector('[data-backend-installed]')?.className).not.toContain('is-set');
  vi.mocked(readAuthStatus).mockResolvedValue([]);
  fireEvent.click(screen.getByRole('button',{name:'Refresh authorization status'}));
  await waitFor(()=>expect(document.querySelector('[data-provider-configured="github"]')).toBeNull());
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_install_start'||c==='native_backend_start')).toBe(false);
 });
 it('puts Backend/Logs after Comfy and directly exposes backend settings and provider authorization',()=>{
  mount();
  expect([...document.querySelectorAll('.aio-tabs > [role="tab"]')].map(el=>el.textContent)).toEqual(['Studio','Library','Prompt Forge','Chat','Media Analysis','TVE','Comfy','Monitoring','Run Plan','Backend','Logs']);
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));
  expect(screen.getByLabelText('Language')).toBeTruthy();
  expect(screen.getByRole('region',{name:'Provider authorization'})).toBeTruthy();
  expect(screen.queryByRole('button',{name:'AiO installer'})).toBeNull();
  expect(screen.queryByRole('button',{name:'Provider authorization'})).toBeNull();
  expect(screen.queryByRole('button',{name:'Settings'})).toBeNull();
 });
 it('opens Comfy beside Library without starting a backend and preserves Studio',()=>{
  render(<I18nProvider><AiOSetup studio={<input aria-label="Studio draft" defaultValue="keep me"/>} onConnectExisting={vi.fn()}/></I18nProvider>);
  fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
  const draft=screen.getByLabelText('Studio draft');
  const tabs=screen.getAllByRole('tab');expect(tabs.map(x=>x.textContent)).toEqual(['Studio','Library','Prompt Forge','Chat','Media Analysis','TVE','Comfy','Monitoring','Run Plan','Backend','Logs']);
  expect([...document.querySelectorAll('.aio-utility-tabs > [role="tab"]')].map(el=>el.textContent)).toEqual(['Monitoring','Run Plan','Backend','Logs']);
  fireEvent.click(screen.getByRole('tab',{name:'Comfy'}));
  expect(document.querySelector('.comfy-loading')?.textContent).toContain('Waiting for backend');
  expect(document.querySelector('.comfy-loading .aio-spinner')).not.toBeNull();
  expect(screen.getByRole('tab',{name:'Backend'}).closest('.aio-tabs')).not.toBeNull();
  expect(screen.queryByLabelText('Studio draft')).not.toBeNull();
  expect(draft.closest('[hidden]')).not.toBeNull();
  expect(document.querySelector('iframe')).toBeNull();
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
  expect(screen.getByLabelText('Studio draft')).toBe(draft);
  expect(draft.closest('[hidden]')).toBeNull();
  expect(screen.getByRole('tab',{name:'Backend'})).toBeTruthy();
 });
 it('loads only the owned running endpoint and keeps the editor mounted across tab switches',async()=>{
  nativeFixture();
  vi.mocked(invoke).mockImplementation(async(c,args)=>{if(c==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(c==='native_llm_read')return defaultLlmConfig;if(c==='native_llm_models')return {models:[],warnings:[]};
   if(c==='native_install_defaults')return defaults;
   if(c==='native_install_plan')return plan((args as {request:NativeInstallRequest}).request);
   if(c==='native_install_start')return {state:'succeeded',logs:[],error:null};
   if(c==='native_backend_stop')return {state:'stopped',endpoint:null,logs:[],error:null,generationReady:false};
   if(c==='native_backend_status')return {state:'running',endpoint:'http://127.0.0.1:49312',logs:[],error:null,generationReady:false};
   if(c==='native_download_catalog')return {version:1,providers:[],entries:[]};
   throw Error('Unexpected IPC '+c);
  });
  mount();open();await screen.findByRole('button',{name:'Start install'});click('Start install');
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));await waitFor(()=>expect(document.querySelector('[data-backend-installed]')?.className).toContain('is-set'));
  fireEvent.click(screen.getByRole('tab',{name:'Comfy'}));
  expect(document.querySelector('iframe')).toBeNull();
  fireEvent.click(screen.getByRole('tab',{name:'Studio'}));click('Backend');click('Test');
  await waitFor(()=>expect(screen.queryByRole('button',{name:'Stop'})).not.toBeNull());
  await waitFor(()=>expect(document.querySelector('[data-backend-state]')?.textContent).toContain('Started'));
  fireEvent.click(screen.getByRole('tab',{name:'Comfy'}));
  const frame=screen.getByTitle('ComfyUI workflow editor');
  expect(frame.getAttribute('src')).toBe('http://127.0.0.1:49312');
  expect(document.querySelector('.comfy-loading')?.textContent).toContain('Loading ComfyUI');
  fireEvent.load(frame);expect(document.querySelector('.comfy-loading')).toBeNull();
  expect(frame.getAttribute('sandbox')).not.toContain('allow-top-navigation');
  fireEvent.click(screen.getByRole('tab',{name:'Library'}));
  expect(frame.closest('[hidden]')).not.toBeNull();
  fireEvent.click(screen.getByRole('tab',{name:'Comfy'}));
  expect(screen.getByTitle('ComfyUI workflow editor')).toBe(frame);
  expect(frame.closest('[hidden]')).toBeNull();
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_backend_start'||c==='native_backend_stop')).toBe(false);
  fireEvent.click(screen.getByRole('tab',{name:'Studio'}));click('Backend');click('Stop');
  await waitFor(()=>expect(document.querySelector('iframe')).toBeNull());
  fireEvent.click(screen.getByRole('tab',{name:'Comfy'}));
  expect(document.querySelector('.comfy-loading')?.textContent).toContain('Waiting for backend');
 });
 it('autostarts after successful fresh provisioning but never before install consent',async()=>{
  nativeFixture();localStorage.removeItem('deas.pref.backend.autostart');
  vi.mocked(invoke).mockImplementation(async(c,args)=>{if(c==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(c==='native_llm_read')return defaultLlmConfig;if(c==='native_llm_models')return {models:[],warnings:[]};
   if(c==='native_install_defaults')return defaults;
   if(c==='native_install_plan')return plan((args as {request:NativeInstallRequest}).request);
   if(c==='native_install_start')return {state:'succeeded',stepId:null,logs:[],error:null};
   return {state:c==='native_backend_start'?'starting':'stopped',endpoint:null,logs:[],error:null,generationReady:false};
  });
  mount();open();await screen.findByRole('button',{name:'Start install'});
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_backend_start')).toBe(false);
  click('Start install');await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_backend_start'));
  expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_backend_start')).toHaveLength(1);
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));await waitFor(()=>expect(document.querySelector('[data-backend-state]')?.textContent).toContain('Starting…'));
 });
 it.each(['default','running','stopped'])('restores the checked environment respecting one-shot update state: %s',async resume=>{
  nativeFixture();localStorage.setItem('deas.pref.backend.autostart',resume==='running'?'false':'true');localStorage.setItem('deas.pref.install.root','"/custom/env"');
  if(resume!=='default')localStorage.setItem('deas.pref.backend.resumeAfterUpdate',JSON.stringify(resume));
  vi.mocked(invoke).mockImplementation(async c=>{if(c==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(c==='native_llm_read')return defaultLlmConfig;if(c==='native_llm_models')return {models:[],warnings:[]};
   if(c==='native_files_present')return [true];
   if(c==='native_install_defaults')return defaults;
   if(c==='native_install_plan')return {...plan({...request,storagePath:'/custom/env'}),id:'reopen',reopenId:'reopen'};
   if(c==='native_settings_read')return {};
   if(c==='native_install_open')return {state:'succeeded',stepId:null,logs:[],error:null};
   return {state:c==='native_backend_start'?'starting':'stopped',endpoint:null,logs:[],error:null,generationReady:false};
  });
  mount();await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_install_open',{planId:'reopen',confirmed:true}));
  if(resume==='stopped'){await screen.findByRole('tab',{name:'Studio',selected:true});await act(async()=>{await new Promise(r=>setTimeout(r,20));});expect(invoke).not.toHaveBeenCalledWith('native_backend_start');}
  else await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_backend_start'));
  expect(localStorage.getItem('deas.pref.backend.resumeAfterUpdate')).toBeNull();
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_install_start')).toBe(false);
 });
 it('defaults backend autostart on and remembers the Settings switch being disabled',async()=>{
  localStorage.removeItem('deas.pref.backend.autostart');const first=mount();click('Backend');
  const toggle=screen.getByRole('switch',{name:'Auto-start backend'});expect(toggle.getAttribute('aria-checked')).toBe('true');
  fireEvent.click(toggle);await waitFor(()=>expect(localStorage.getItem('deas.pref.backend.autostart')).toBe('false'));
  first.unmount();mount();click('Backend');expect(screen.getByRole('switch',{name:'Auto-start backend'}).getAttribute('aria-checked')).toBe('false');
 });
 it('recognizes a chosen environment root without nesting another environment directory',async()=>{
  nativeFixture();
  vi.mocked(invoke).mockImplementation(async(c,args)=>{if(c==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(c==='native_llm_read')return defaultLlmConfig;if(c==='native_llm_models')return {models:[],warnings:[]};
   if(c==='native_install_defaults')return defaults;
   if(c==='native_pick_directory')return '/chosen/env';
   if(c==='native_files_present')return [true,true];
   if(c==='native_install_plan'){const r=(args as {request:NativeInstallRequest}).request;return {...plan(r),...(r.storagePath==='/chosen/env'?{reopenId:'fixture-plan'}:{})};}
   return {};
  });
  mount();open();await screen.findByRole('button',{name:'Start install'});click('Choose parent folder…');
  await screen.findByRole('button',{name:'Use existing environment'});expect((screen.getByLabelText('Storage directory') as HTMLInputElement).value).toBe('/chosen/env');
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_install_start'||c==='native_install_open')).toBe(false);
 });
 it('does not autostart an unverified remembered folder or turn it into a new install',async()=>{
  nativeFixture();localStorage.removeItem('deas.pref.backend.autostart');localStorage.setItem('deas.pref.install.root','"/invalid/env"');
  vi.mocked(invoke).mockImplementation(async c=>c==='native_llm_read'?defaultLlmConfig:c==='native_llm_models'?{models:[],warnings:[]}:c==='native_files_present'?[true]:plan());
  mount();await screen.findByText(/Saved environment is not a complete DEAS installation/);
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_install_open'||c==='native_install_start'||c==='native_backend_start')).toBe(false);
 });
 it('opens a validated existing environment instead of reinstalling the selected folder',async()=>{
  nativeFixture();const installed=vi.fn();const existing={...plan({...request,storagePath:'/real/existing-env',features:[]}),id:'reopen-preview',reopenId:'reopen-preview'};
  vi.mocked(invoke).mockImplementation(async(command,args)=>{if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;if(command==='native_llm_models')return {models:[],warnings:[]};
   if(command==='native_install_defaults')return defaults;
   if(command==='native_install_plan')return (args as {request:NativeInstallRequest}).request.storagePath==='/alias/existing-env'?existing:plan();
   if(command==='native_settings_read')return {models:'/saved/models'};
   if(command==='native_install_open')return {state:'succeeded',stepId:null,logs:['opened existing fixture'],error:null};
   return {state:'stopped',endpoint:null,logs:[],error:null,generationReady:false};
  });
  mount(vi.fn(),installed);open();await screen.findByRole('button',{name:'Start install'});
  change('Storage directory','/alias/existing-env');await screen.findByRole('button',{name:'Use existing environment'});
  expect(installed).not.toHaveBeenCalled();expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_install_open')).toBe(false);
  click('Use existing environment');await waitFor(()=>expect(installed).toHaveBeenCalledWith([]));
  expect(invoke).toHaveBeenCalledWith('native_install_open',{planId:'reopen-preview',confirmed:true});
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_install_start'||c==='native_backend_start')).toBe(false);
  expect(JSON.parse(localStorage.getItem('deas.pref.install.root')!)).toBe('/real/existing-env');
 });
 it('hands off exact successful native features once, without starting or inspecting a backend',async()=>{
  nativeFixture();const installed=vi.fn();let prepared=plan();
  vi.mocked(invoke).mockImplementation(async(command,args)=>{if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;if(command==='native_llm_models')return {models:[],warnings:[]};
   if(command==='native_install_defaults')return defaults;
   if(command==='native_install_plan'){prepared=plan((args as {request:NativeInstallRequest}).request);return prepared;}
   return {state:'succeeded',stepId:null,logs:['installed fixture'],error:null};
  });
  const view=mount(vi.fn(),installed);open();await screen.findByText('CPU fallback fixture');
  await screen.findByRole('button',{name:'Start install'});expect(installed).not.toHaveBeenCalled();
  click('Start install');await waitFor(()=>expect(installed).toHaveBeenCalledTimes(1));
  expect(installed.mock.calls[0][0]).toBe(prepared.request.features);
  expect(installed.mock.calls[0][0]).toEqual(['director','forge','refmods','continuity']);
  view.rerender(<I18nProvider><AiOSetup onConnectExisting={vi.fn()} onInstalled={features=>installed(features)}/></I18nProvider>);
  expect(installed).toHaveBeenCalledTimes(1);
  expect(vi.mocked(invoke).mock.calls.map(([command])=>command).filter(c=>c!=='native_theme_packs'&&c!=='native_spellcheck_status'&&c!=='native_auth_status'&&c!=='native_repo_stars'&&c!=='native_update_check')).toEqual(['native_install_defaults','native_install_plan','native_install_start']);
 });
 it.each(['idle','failed','cancelled'] as const)('does not hand off a native %s install',async state=>{
  nativeFixture();const installed=vi.fn();
  vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:command==='native_install_plan'?plan((args as {request:NativeInstallRequest}).request):{state,stepId:null,logs:['not succeeded'],error:null});
  mount(vi.fn(),installed);open();await screen.findByText('CPU fallback fixture');await screen.findByRole('button',{name:'Start install'});click('Start install');await screen.findByText('not succeeded');
  expect(installed).not.toHaveBeenCalled();
 });
 it.each(['failed','cancelled'] as const)('re-plans after a %s install instead of reusing its consumed plan',async state=>{
  nativeFixture();let id=0;const started:string[]=[];
  vi.mocked(invoke).mockImplementation(async(command,args)=>{if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;if(command==='native_llm_models')return {models:[],warnings:[]};
   if(command==='native_install_defaults')return defaults;
   if(command==='native_install_plan')return {...plan((args as {request:NativeInstallRequest}).request),id:`plan-${++id}`};
   if(command==='native_install_start'){started.push((args as {planId:string}).planId);return {state,stepId:null,logs:['attempt ended'],error:state==='failed'?'uv download stalled':null};}
   return {state,stepId:null,logs:[],error:null};
  });
  mount();open();await screen.findByText('CPU fallback fixture');await screen.findByRole('button',{name:'Start install'});click('Start install');await screen.findByText('attempt ended');
  await waitFor(()=>expect(id).toBe(2));expect(screen.getByText('attempt ended')).toBeTruthy();
  await waitFor(()=>expect((screen.getByRole('button',{name:'Start install'}) as HTMLButtonElement).disabled).toBe(false));
  click('Start install');await waitFor(()=>expect(started).toEqual(['plan-1','plan-2']));
 });
 it.each(['failed','cancelled'] as const)('recovers from a polled %s install only after choosing a new target',async state=>{
  nativeFixture();let id=0;let usedRoot='';const started:string[]=[];const completion=deferred<unknown>();
  vi.mocked(invoke).mockImplementation(async(command,args)=>{
   if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;
   if(command==='native_llm_models')return {models:[],warnings:[]};
   if(command==='native_install_defaults')return defaults;
   if(command==='native_install_plan'){
    const request=(args as {request:NativeInstallRequest}).request;
    if(request.storagePath===usedRoot)throw new Error('unfinished installation fixture');
    return {...plan(request),id:`plan-${++id}`};
   }
   if(command==='native_install_start'){started.push((args as {planId:string}).planId);usedRoot=defaults.storagePath;return {state:'running',stepId:null,logs:['attempt running'],error:null};}
   if(command==='native_install_status')return completion.promise;
   return {state:'idle',stepId:null,logs:[],error:null};
  });
  mount();open();await screen.findByRole('button',{name:'Start install'});click('Start install');await screen.findByText('attempt running');
  await act(async()=>completion.resolve({state,stepId:null,logs:['attempt ended'],error:state==='failed'?'uv download stalled':null}));
  await screen.findByText(/unfinished installation fixture/);expect(screen.getByText('attempt ended')).toBeTruthy();
  expect(started).toEqual(['plan-1']);expect(screen.queryByRole('button',{name:'Start install'})).toBeNull();
  change('Storage directory','/fresh/environment');await screen.findByRole('button',{name:'Start install'});
  expect(screen.getByText('attempt ended')).toBeTruthy();expect(started).toEqual(['plan-1']);
  click('Start install');await waitFor(()=>expect(started).toEqual(['plan-1','plan-2']));
 });
 it('shows a planning error in the review panel where Start would be',async()=>{
  nativeFixture();
  vi.mocked(invoke).mockImplementation(async command=>{if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;if(command==='native_llm_models')return {models:[],warnings:[]};
   if(command==='native_install_defaults')return defaults;
   if(command==='native_install_plan')throw new Error('unfinished installation fixture');
   return {state:'idle',stepId:null,logs:[],error:null};
  });
  mount();open();
  const alert=await screen.findByText(/unfinished installation fixture/);
  expect(alert.closest('.aio-right')).not.toBeNull();
  expect(screen.getAllByText(/unfinished installation fixture/)).toHaveLength(1);
  expect(screen.queryByRole('button',{name:'Start install'})).toBeNull();
 });
 it('hands off only when a running actual install polls succeeded',async()=>{
  nativeFixture();const installed=vi.fn();const completion=deferred<unknown>();
  vi.mocked(invoke).mockImplementation(async(command,args)=>{if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;if(command==='native_llm_models')return {models:[],warnings:[]};
   if(command==='native_install_defaults')return defaults;
   if(command==='native_install_plan')return plan((args as {request:NativeInstallRequest}).request);
   if(command==='native_install_status')return completion.promise;
   return {state:'running',stepId:null,logs:['running install'],error:null};
  });
  mount(vi.fn(),installed);open();await screen.findByText('CPU fallback fixture');await screen.findByRole('button',{name:'Start install'});click('Start install');await screen.findByText('running install');
  expect(installed).not.toHaveBeenCalled();await act(async()=>completion.resolve({state:'succeeded',stepId:null,logs:[],error:null}));
  expect(installed).toHaveBeenCalledExactlyOnceWith(['director','forge','refmods','continuity']);
 });
 it('does not replay a previous success when the installer is reopened',async()=>{
  nativeFixture();const installed=vi.fn();let id=0;
  vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:command==='native_install_plan'?{...plan((args as {request:NativeInstallRequest}).request),id:`plan-${++id}`}:{state:'succeeded',stepId:null,logs:[],error:null});
  mount(vi.fn(),installed);open();await screen.findByText('CPU fallback fixture');await screen.findByRole('button',{name:'Start install'});click('Start install');await waitFor(()=>expect(installed).toHaveBeenCalledTimes(1));
  expect(JSON.parse(localStorage.getItem('deas.pref.install.root')!)).toBe(request.storagePath);expect(screen.queryByLabelText('Director')).toBeNull();open();open();
  // Re-runs moved to Backend: reopening the installer neither shows the review nor replays success.
  expect(screen.queryByLabelText('Director')).toBeNull();expect(screen.queryByRole('button',{name:'Start install'})).toBeNull();
  await new Promise(r=>setTimeout(r,300));expect(installed).toHaveBeenCalledTimes(1);
 });
 it('remembers a successful install root and reopens the Studio while its manifest exists',async()=>{
  nativeFixture();localStorage.setItem('deas.pref.install.root','"/custom/env"');
  vi.mocked(invoke).mockImplementation(async command=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_files_present'?[true]:command==='native_settings_read'?{models:'/custom/models'}:command==='native_install_plan'?{...plan({...request,storagePath:'/custom/env'}),id:'reopen',reopenId:'reopen'}:command==='native_install_open'?{state:'succeeded',stepId:null,logs:[],error:null}:defaults);
  mount();await screen.findByRole('tab',{name:'Studio',selected:true});
  expect(invoke).toHaveBeenCalledWith('native_files_present',{paths:['/custom/env/deas-install.json']});
  open();await screen.findByRole('button',{name:'Start'});expect(invoke).toHaveBeenCalledWith('native_install_open',{planId:'reopen',confirmed:true});expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_backend_start'||c==='native_install_start')).toBe(false);expect(screen.queryByRole('button',{name:'Run install again'})).toBeNull();
 });
 it('forgets a vanished install root and keeps the first run',async()=>{
  nativeFixture();localStorage.setItem('deas.pref.install.root','"/gone/env"');
  vi.mocked(invoke).mockImplementation(async command=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_files_present'?[false]:defaults);
  mount();await waitFor(()=>expect(localStorage.getItem('deas.pref.install.root')).toBeNull());
  expect(screen.getByRole('tab',{name:'Backend'}).getAttribute('aria-selected')).toBe('true');expect(screen.queryByText(/Installed in/)).toBeNull();
 });
 it('opens every editable field without inventing a browser path or hardware',()=>{
  mount();open();expect((screen.getByLabelText('Storage directory') as HTMLInputElement).value).toBe('');
  expect((screen.getByLabelText('Backend') as HTMLSelectElement).value).toBe('');
  expect(screen.getByLabelText('ComfyUI Git ref')).toBeTruthy();
  expect((screen.getByLabelText('Director') as HTMLInputElement).checked).toBe(true);
  for(const label of ['SageAttention (opt-in)','Radial attention (opt-in)','FlashAttention (opt-in)','FFmpeg (opt-in)'])expect((screen.getByLabelText(label) as HTMLInputElement).checked).toBe(false);
  expect(screen.queryByRole('button',{name:'Confirm & continue'})).toBeNull();expect(invoke).not.toHaveBeenCalled();expect(screen.queryByRole('button',{name:'Start install'})).toBeNull();
 });
 it('reads defaults on the initial Backend view; automatic planning never starts installation; final click confirms once',async()=>{
  nativeFixture();mount();expect(vi.mocked(invoke).mock.calls.filter(([c])=>c!=='native_theme_packs'&&c!=='native_spellcheck_status'&&c!=='native_auth_status'&&c!=='native_repo_stars'&&c!=='native_update_check')).toEqual([['native_install_defaults',{}]]);open();await screen.findByText('CPU fallback fixture');
  expect(invoke).toHaveBeenCalledWith('native_install_defaults',{});
  await screen.findByRole('button',{name:'Start install'});
  expect(vi.mocked(invoke).mock.calls.map(([c])=>c).filter(c=>c!=='native_theme_packs'&&c!=='native_spellcheck_status'&&c!=='native_auth_status'&&c!=='native_repo_stars'&&c!=='native_update_check')).toEqual(['native_install_defaults','native_install_plan']);expect(invoke).toHaveBeenCalledWith('native_install_plan',{request});expect(invoke).not.toHaveBeenCalledWith('native_install_start',expect.anything());
  const button=screen.getByRole('button',{name:'Start install'});fireEvent.click(button);fireEvent.click(button);
  await screen.findByText('native fixture log');expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_install_start')).toEqual([['native_install_start',{planId:'fixture-plan',confirmed:true}]]);
  expect(screen.getByLabelText('Storage directory').matches(':disabled')).toBe(true);
 });
 it('preserves all edited options and expands Director in the immutable request',async()=>{
  nativeFixture();await ready();fireEvent.click(screen.getByLabelText('Director'));
  expect(screen.queryByRole('button',{name:'Start install'})).toBeNull();
  for(const label of ['SageAttention (opt-in)','Radial attention (opt-in)','FlashAttention (opt-in)','FFmpeg (opt-in)'])fireEvent.click(screen.getByLabelText(label));
  change('ComfyUI Git ref','v0.3.50');change('Storage directory','/other/new');change('Backend','nvidia');
  await screen.findByRole('button',{name:'Start install'});
  expect(invoke).toHaveBeenLastCalledWith('native_install_plan',{request:{storagePath:'/other/new',backend:'nvidia',features:[],options:{sage:true,radial:true,flash:true,ffmpeg:true},comfyRef:'v0.3.50'}});
 });
 it('invalid text is visible, disables review, and invalidates the previous plan',async()=>{
  nativeFixture();await ready();change('Storage directory','relative');expect(screen.queryByRole('button',{name:'Start install'})).toBeNull();
  change('Storage directory','/new');change('ComfyUI Git ref','bad ref');await new Promise(r=>setTimeout(r,400));expect(screen.queryByRole('button',{name:'Start install'})).toBeNull();expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_install_plan')).toHaveLength(1);
 });
 it('late defaults prefill only untouched fields, never overwrite even a cleared manual field',async()=>{
  nativeFixture();const late=deferred<typeof defaults>();vi.mocked(invoke).mockImplementation(async()=>late.promise);
  mount();open();change('Storage directory','/manual');change('Storage directory','');change('Backend','amd');
  await act(async()=>late.resolve(defaults));expect((screen.getByLabelText('Storage directory') as HTMLInputElement).value).toBe('');expect((screen.getByLabelText('Backend') as HTMLSelectElement).value).toBe('amd');
 });
 it('late defaults do not replace a prepared request',async()=>{
  nativeFixture();const late=deferred<typeof defaults>();vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?late.promise:plan((args as {request:NativeInstallRequest}).request));
  mount();open();change('Storage directory','/manual');change('Backend','cpu');await screen.findByRole('button',{name:'Start install'});
  await act(async()=>late.resolve({...defaults,storagePath:'/late',backend:'nvidia'}));expect(screen.getByLabelText('Storage directory')).toHaveProperty('value','/manual');expect((screen.getByLabelText('Backend') as HTMLSelectElement).value).toBe('cpu');
 });
 it('failed defaults leave manual editing available without guessing',async()=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async c=>{if(c==='native_spellcheck_status')return null;throw 'probe unavailable';});mount();open();await screen.findByRole('alert');expect((screen.getByLabelText('Backend') as HTMLSelectElement).value).toBe('');expect((screen.getByLabelText('Storage directory') as HTMLInputElement).value).toBe('');change('Storage directory','/manual');change('Backend','cpu');await waitFor(()=>expect(screen.getAllByRole('alert')).toHaveLength(2));expect(screen.queryByRole('button',{name:'Start install'})).toBeNull();
 });
 it('shows blockers and original native errors without permitting start',async()=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async command=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:{...plan(),canExecute:false,unsupportedOptions:['sage unsupported'],warnings:['target collision']});await ready();expect(screen.getByText('target collision')).toBeTruthy();expect(screen.getByText('sage unsupported')).toBeTruthy();expect((screen.getByRole('button',{name:'Start install'}) as HTMLButtonElement).disabled).toBe(true);
 });
 it('refuses mismatched returned requests even when canExecute is true',async()=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async command=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:plan({...request,storagePath:'/unexpected'}));await ready();
  expect((screen.getByRole('button',{name:'Start install'}) as HTMLButtonElement).disabled).toBe(true);click('Start install');expect(invoke).not.toHaveBeenCalledWith('native_install_start',expect.anything());
 });
 it('blocks unsupported options even when canExecute is true',async()=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async command=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:{...plan(),unsupportedOptions:['sage unsupported']});await ready();expect((screen.getByRole('button',{name:'Start install'}) as HTMLButtonElement).disabled).toBe(true);
 });
 it('plans once per request and locks edits until the native plan returns',async()=>{
  nativeFixture();const late=deferred<ReturnType<typeof plan>>();vi.mocked(invoke).mockImplementation(async command=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:late.promise);
  mount();open();await screen.findByText('CPU fallback fixture');await waitFor(()=>expect(screen.getByLabelText('Backend').matches(':disabled')).toBe(true));await new Promise(r=>setTimeout(r,400));
  expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_install_plan')).toHaveLength(1);
  await act(async()=>late.resolve(plan()));expect(screen.getByLabelText('Backend').matches(':disabled')).toBe(false);
 });
 it('late defaults cannot change or unlock a running request',async()=>{
  nativeFixture();const late=deferred<typeof defaults>();vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?late.promise:command==='native_install_plan'?plan((args as {request:NativeInstallRequest}).request):{state:'running',stepId:'python',logs:['running fixture'],error:null});
  mount();open();change('Storage directory','/manual/new');change('Backend','cpu');await screen.findByRole('button',{name:'Start install'});click('Start install');await screen.findByText('running fixture');
  await act(async()=>late.resolve({...defaults,storagePath:'/late',backend:'nvidia'}));expect((screen.getByLabelText('Storage directory') as HTMLInputElement).value).toBe('/manual/new');expect((screen.getByLabelText('Backend') as HTMLSelectElement).value).toBe('cpu');expect(screen.getByLabelText('Backend').matches(':disabled')).toBe(true);
 });
 it('names the detected device in the one-line plan summary before authorization',async()=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async command=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:{...plan(),hardware:{name:'GPU fixture',uuid:'GPU-fixture'},torch:{packages:['torch==fixture'],indexUrl:'https://index.fixture',cuda:'fixture'}});await ready();expect(screen.getByText(/GPU fixture/)).toBeTruthy();
 });
 it('uses a native parent chooser without writing and preserves values on cancellation',async()=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:command==='native_install_plan'?plan((args as {request:NativeInstallRequest}).request):null);mount();open();await screen.findByText('CPU fallback fixture');change('Storage directory','/manual/new');click('Choose parent folder…');await waitFor(()=>expect((screen.getByRole('button',{name:'Choose parent folder…'}) as HTMLButtonElement).disabled).toBe(false));expect((screen.getByLabelText('Storage directory') as HTMLInputElement).value).toBe('/manual/new');expect(invoke).toHaveBeenCalledWith('native_pick_directory',{title:'Choose an existing parent folder'});
 });
 it('joins a native parent into a new target and lets the user edit it before review',async()=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:command==='native_install_plan'?plan((args as {request:NativeInstallRequest}).request):'/selected');mount();open();await screen.findByText('CPU fallback fixture');click('Choose parent folder…');await waitFor(()=>expect((screen.getByLabelText('Storage directory') as HTMLInputElement).value).toBe('/selected/deas-environment'));change('Storage directory','/selected/custom-new');expect((screen.getByLabelText('Storage directory') as HTMLInputElement).value).toBe('/selected/custom-new');expect(invoke).not.toHaveBeenCalledWith('native_install_start',expect.anything());
 });
 it('includes optional folder choices in a fresh immutable install plan without saving or moving data',async()=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:command==='native_install_plan'?plan((args as {request:NativeInstallRequest}).request):command==='native_pick_directory'?'/real/models':null);
  await ready();click('Choose Models folder');await waitFor(()=>expect(screen.getByText('/real/models')).toBeTruthy());
  await waitFor(()=>expect(invoke).toHaveBeenLastCalledWith('native_install_plan',{request:{...request,folders:{models:'/real/models'}}}));
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_settings_write'||c==='native_settings_move'||c==='native_install_start')).toBe(false);
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));expect(screen.getAllByText('/real/models')).toHaveLength(1);expect(screen.queryByRole('button',{name:'Save folders'})).toBeNull();
 });
 it.each([{keys:['models']},{keys:['output','models','input']}])('can start after choosing folders in $keys order and a native serialization round trip',async({keys})=>{
  nativeFixture();let selected='';let id=0;
  vi.mocked(invoke).mockImplementation(async(command,args)=>{if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;if(command==='native_llm_models')return {models:[],warnings:[]};
   if(command==='native_install_defaults')return defaults;
   if(command==='native_pick_directory')return `/real/${selected}`;
   if(command==='native_install_plan'){
    const r=(args as {request:NativeInstallRequest}).request;
    const folders=r.folders?Object.fromEntries(['models','input','output'].filter(key=>r.folders?.[key as keyof typeof r.folders]).map(key=>[key,r.folders![key as keyof typeof r.folders]])):undefined;
    return {...plan({...r,...(folders?{folders}:{})}),id:`plan-${++id}`};
   }
   return {state:'running',stepId:'python',logs:[],error:null};
  });
  await ready();
  for(const key of keys){
   selected=key;click(`Choose ${key==='models'?'Models':key==='input'?'Input':'Output'} folder`);
   await waitFor(()=>expect((screen.getByRole('button',{name:'Start install'}) as HTMLButtonElement).disabled).toBe(false));
  }
  const calls=vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_install_plan');
  const latestId=`plan-${calls.length}`;
  click('Start install');await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_install_start',{planId:latestId,confirmed:true}));
 });
 it.each(['models','input','output'] as const)('refuses a native plan with a different %s folder',async(key)=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async(command,args)=>{if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;if(command==='native_llm_models')return {models:[],warnings:[]};
   if(command==='native_install_defaults')return defaults;
   if(command==='native_pick_directory')return '/chosen';
   if(command==='native_install_plan'){
    const r=(args as {request:NativeInstallRequest}).request;
    return plan(r.folders?{...r,folders:{...r.folders,[key]:'/unexpected'}}:r);
   }
   return null;
  });
  await ready();click(`Choose ${key==='models'?'Models':key==='input'?'Input':'Output'} folder`);
  await waitFor(()=>expect(invoke).toHaveBeenLastCalledWith('native_install_plan',{request:{...request,folders:{[key]:'/chosen'}}}));
  expect((screen.getByRole('button',{name:'Start install'}) as HTMLButtonElement).disabled).toBe(true);
  click('Start install');expect(invoke).not.toHaveBeenCalledWith('native_install_start',expect.anything());
 });
 it('hands installer-selected folders to Studio after asynchronous provisioning without reopening Settings',async()=>{
  nativeFixture();const environment=vi.fn();let running=false;
  const folders={models:'/chosen/models',input:'/chosen/input',output:'/chosen/output'};
  vi.mocked(invoke).mockImplementation(async(command,args)=>{
   if(command==='native_install_defaults')return defaults;
   if(command==='native_install_plan')return plan((args as {request:NativeInstallRequest}).request);
   if(command==='native_pick_directory')return folders[choosing];
   if(command==='native_install_start'){running=true;return {state:'running',stepId:'verify',logs:[],error:null};}
   if(command==='native_install_status')return {state:running?'succeeded':'idle',stepId:null,logs:[],error:null};
   if(command==='native_backend_test')return DEPS;
   if(command==='native_backend_status')return {state:'stopped',endpoint:null,error:null,generationReady:false};
   if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;
   if(command==='native_llm_models')return {models:[],warnings:[]};
   throw Error('Unexpected fixture IPC '+command);
  });
  let choosing:keyof typeof folders='models';
  render(<I18nProvider><AiOSetup onConnectExisting={vi.fn()} onEnvironment={environment}/></I18nProvider>);
  await screen.findByRole('button',{name:'Start install'});
  for(const key of ['models','input','output'] as const){choosing=key;click(`Choose ${key==='models'?'Models':key==='input'?'Input':'Output'} folder`);await waitFor(()=>expect((screen.getByRole('button',{name:'Start install'}) as HTMLButtonElement).disabled).toBe(false));}
  click('Start install');await screen.findByRole('tab',{name:'Studio',selected:true});
  await waitFor(()=>expect(environment).toHaveBeenLastCalledWith({root:request.storagePath,folders}));
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_settings_read'||c==='native_settings_write'||c==='native_settings_move')).toBe(false);
 });
 it('saving installed folders preserves successful install state without replanning an existing environment',async()=>{
  nativeFixture();vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_defaults'?defaults:command==='native_install_plan'?plan((args as {request:NativeInstallRequest}).request):command==='native_settings_read'?{}:command==='native_pick_directory'?'/new/models':command==='native_settings_write'?(args as {folders:unknown}).folders:{state:'succeeded',stepId:null,logs:[],error:null});
  await ready();click('Start install');await screen.findByRole('tab',{name:'Studio',selected:true});
  // The success handoff switches to Studio in an effect; let it land before navigating, or it overrides the click.
  await screen.findByRole('tab',{name:'Studio',selected:true});
  const before=vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_install_plan').length;
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));
  const settings=screen.getByRole('region',{name:'Settings'});
  await waitFor(()=>expect((settings.querySelector('button') as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(settings.querySelector('button')!);await screen.findByText('/new/models');click('Save folders');
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_settings_write',{root:request.storagePath,folders:{models:'/new/models'}}));
  await new Promise(r=>setTimeout(r,350));expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_install_plan')).toHaveLength(before);
  expect(screen.getByText('Installed')).toBeTruthy();
 });
 it('cancels only the native installation and shows returned status, not simulated progress',async()=>{
  nativeFixture();await ready();click('Start install');await screen.findByText('native fixture log');
  vi.mocked(invoke).mockImplementation(async command=>command==='native_llm_read'?defaultLlmConfig:command==='native_llm_models'?{models:[],warnings:[]}:command==='native_install_cancel'?null:{state:'cancelled',stepId:null,logs:['native cancellation fixture'],error:null});click('Cancel installation');await screen.findByText('native cancellation fixture');expect(invoke).toHaveBeenCalledWith('native_install_cancel');expect(screen.getAllByRole('status').some(el=>el.textContent==='Cancelled')).toBe(true);
 });
 it('discards defaults after unmount and reuses one request during StrictMode replay',async()=>{
  nativeFixture();const late=deferred<typeof defaults>();vi.mocked(invoke).mockImplementation(async()=>late.promise);
  const first=mount();open();first.unmount();await act(async()=>late.resolve(defaults));
  vi.mocked(invoke).mockResolvedValue(defaults);render(<StrictMode><I18nProvider><AiOSetup onConnectExisting={vi.fn()}/></I18nProvider></StrictMode>);open();await screen.findByText('CPU fallback fixture');expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_install_defaults')).toHaveLength(2);
 });
 it('connects an existing backend only through an explicit valid address and never installs',()=>{
  const connect=vi.fn();mount(connect);open();
  change('Existing backend address','not a url');click('Connect existing backend');expect(connect).not.toHaveBeenCalled();expect(screen.getByRole('alert')).toBeTruthy();
  change('Existing backend address','http://127.0.0.1:8188');click('Connect existing backend');expect(connect).toHaveBeenCalledWith('http://127.0.0.1:8188',true);expect(invoke).not.toHaveBeenCalled();
  expect(screen.queryByRole('button',{name:/Manual|Save local draft|Restore/})).toBeNull();
 });
 it('takeover rebuilds a foreign folder only after an explicit confirm and marks the request',async()=>{
  nativeFixture();
  vi.mocked(invoke).mockImplementation(async(c,a)=>c==='native_pick_directory'?'/foreign/env':c==='native_files_present'?[true,false]:(async(command,args)=>{if(command==='native_install_defaults')return defaults;if(command==='native_install_plan')return plan((args as {request:NativeInstallRequest}).request);if(command==='native_detect_hardware')return {backend:null};
  if(command==='native_backend_test')return DEPS;return {state:'running',stepId:'python',logs:[],error:null};})(c,a));
  mount();open();
  await screen.findByRole('button',{name:'Start install'});
  fireEvent.click(screen.getByRole('button',{name:'Adopt a foreign installation (rebuild its venv)'}));
  expect(screen.getByText(/deletes and rebuilds/)).toBeTruthy();
  expect(screen.queryByRole('button',{name:'Adopt installation'})).toBeNull(); // no path chosen yet
  fireEvent.click(screen.getByRole('button',{name:'Choose folder'}));
  const adopt=await screen.findByRole('button',{name:'Adopt installation'});
  expect(adopt).toBeTruthy(); // plan re-validates after the path change
  await waitFor(()=>expect((screen.getByRole('button',{name:'Adopt installation'}) as HTMLButtonElement).disabled).toBe(false));
  expect(screen.getByText(/deletes and rebuilds/)).toBeTruthy(); // the plan itself warns about the venv rebuild
  fireEvent.click(screen.getByRole('button',{name:'Adopt installation'}));
  expect(screen.getByRole('alertdialog',{name:'Adopt this installation?'})).toBeTruthy();
  expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_install_start')).toBe(false);
  fireEvent.click(screen.getByRole('button',{name:'Adopt'}));
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_install_start',{planId:'fixture-plan',confirmed:true}));
  const requests=vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_install_plan').map(([,a])=>(a as {request:NativeInstallRequest}).request);
  expect(requests.some(r=>r.takeover===true&&r.storagePath==='/foreign/env')).toBe(true);
 });
});

it('numbers the next folder after an unfinished install',()=>{
 expect(nextFolder('C:\deas-environment')).toBe('C:\deas-environment-2');
 expect(nextFolder('/env/deas-environment-2/')).toBe('/env/deas-environment-3');
});

it.each(['beginner','advanced'] as const)('keeps the %s experience switch only inside Quick Settings',async experience=>{
 const onExperience=vi.fn();
 render(<I18nProvider><AiOSetup experience={experience} onExperience={onExperience} onConnectExisting={vi.fn()}/></I18nProvider>);
 expect(document.querySelector('.experience-switch')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'Quick Settings'}));
 const switches=document.querySelectorAll('.experience-switch');
 expect(switches).toHaveLength(1);
 expect(switches[0].parentElement?.id).toBe('studio-quick-actions');
 expect(switches[0].nextElementSibling).toBe(screen.getByRole('button',{name:'Reset layout'}));
 const next=experience==='beginner'?'Advanced':'Beginner';
 fireEvent.click(screen.getByRole('switch',{name:'Advanced'}));
 expect(onExperience).toHaveBeenCalledWith(next.toLowerCase());
});

it('beginner navigation uses the shared tab band without a separate wordmark',async()=>{
 const onBeginnerPage=vi.fn();
 render(<I18nProvider><AiOSetup experience='beginner' onExperience={vi.fn()} beginnerPage='generate' onBeginnerPage={onBeginnerPage} onConnectExisting={vi.fn()} onInstalled={vi.fn()}/></I18nProvider>);
 const tabs=[...document.querySelectorAll('.aio-tabs > [role="tab"]')];
 expect(tabs.map(el=>el.getAttribute('data-beginner-tab'))).toEqual(['generate','models','collection',null]);
 expect(tabs.every(el=>el.classList.contains('aio-tab'))).toBe(true);
 expect(document.querySelector('.beginner-app-logo')).toBeNull();
 fireEvent.click(tabs[1]);expect(onBeginnerPage).toHaveBeenCalledWith('models');
 await waitFor(()=>expect(tabs[0].getAttribute('aria-selected')).toBe('true'));
 const backend=screen.getByRole('tab',{name:'Backend'});
 expect(backend.closest('.beginner-backend-tabs')).not.toBeNull();
 fireEvent.click(backend);
 await waitFor(()=>expect(backend.getAttribute('aria-selected')).toBe('true'));
 expect(tabs.slice(0,3).every(el=>el.getAttribute('aria-selected')==='false')).toBe(true);
});

it('a first-run beginner without an installed environment lands on the backend wizard',async()=>{
 const onExperience=vi.fn();
 const view=render(<I18nProvider><AiOSetup experience='unset' onExperience={onExperience} onConnectExisting={vi.fn()} onInstalled={vi.fn()}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:/A beginner/}));
 expect(onExperience).toHaveBeenCalledWith('beginner');
 view.rerender(<I18nProvider><AiOSetup experience='beginner' onExperience={onExperience} onConnectExisting={vi.fn()} onInstalled={vi.fn()}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('.backend-overview')).not.toBeNull());
 expect(document.querySelector('.aio-studio-host')).toBeNull();
});
