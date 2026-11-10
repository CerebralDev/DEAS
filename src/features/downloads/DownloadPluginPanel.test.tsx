import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import { DownloadPanel, AuthPanel } from './DownloadPluginPanel';
import { DownloadWorkspace } from './DownloadWorkspace';
import { browseDownloads, planDownloads, setAuthToken, removeAuthToken } from '../../lib/downloads';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:vi.fn(()=>true)}));
const resource={id:'file',label:'Remote file',kind:'file',url:'https://example.invalid/file',downloadable:true,size:23};
const catalog={version:1,providers:[{id:'fixture',label:'Fixture provider',kind:'http',origin:'https://example.invalid',scopes:['index','repo'],anonymousNotice:'Fixture limits'}],entries:[{id:'curated',label:'Curated index',providerId:'fixture',scope:'index',locator:'https://example.invalid/index',kind:'file',target:'models'}]};
const mount=(auth=false)=>render(<I18nProvider>{auth?<AuthPanel/>:<DownloadPanel/>}</I18nProvider>);
const click=(name:string)=>fireEvent.click(screen.getByRole('button',{name}));
beforeEach(()=>{localStorage.clear();localStorage.setItem('deas.locale.v1','en');vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockImplementation(async c=>{
 if(c==='native_download_catalog')return catalog;
 if(c==='native_download_browse')return {resources:[resource,{id:'installed',label:'Installed model',kind:'model',downloadable:false}],nextCursor:'page2',warnings:['Bounded listing']};
 if(c==='native_pick_directory')return '/parent';
 if(c==='native_download_plan')return {id:'immutable',target:'/parent/new',resources:[resource],totalKnownBytes:23,unknownSizeCount:0,warnings:[],canExecute:true};
 if(c==='native_auth_verify')return {providerId:'huggingface',status:'unavailable'};
 if(c==='native_auth_status')return [{providerId:'huggingface',configured:false,persisted:false}];
 if(c==='native_auth_set'||c==='native_auth_remove')return {providerId:'huggingface',configured:c==='native_auth_set',persisted:false};
 return {state:c==='native_download_cancel'?'cancelled':'running',completed:0,total:1,bytesDownloaded:12,error:null};
});});
afterEach(()=>{cleanup();vi.resetAllMocks();vi.useRealTimers();});
async function open(){mount();click('Library');await screen.findByRole('option',{name:'Fixture provider'});}
async function browse(){await open();fireEvent.change(screen.getByLabelText('Curated source'),{target:{value:'curated'}});click('Browse resources');await screen.findByLabelText('Remote file');}
async function pick(){await browse();fireEvent.click(screen.getByLabelText('Remote file'));click('Choose existing parent folder');await screen.findByText('/parent');fireEvent.change(screen.getByLabelText('New subfolder'),{target:{value:'new'}});}
const tab=(name:string)=>fireEvent.click(screen.getByRole('tab',{name}));
it('embedded targeted offer browses once, filters exact filename and waits for Library consent',async()=>{
 const selected={...resource,id:'wanted',label:'wanted.safetensors',locator:'nested/wanted.safetensors'};
 vi.mocked(invoke).mockImplementation(async c=>c==='native_download_catalog'?catalog:c==='native_auth_status'?[]:c==='native_download_browse'?{resources:[selected,{...resource,id:'other',label:'other.pth',locator:'other.pth'}],warnings:[],nextCursor:null}:c==='native_files_present'?[false]:c==='native_download_plan'?{id:'p',target:'/configured/latent_upscale_models',resources:[selected],canExecute:true,warnings:[],totalKnownBytes:23,unknownSizeCount:0}:{state:c==='native_download_status'?'idle':'running',completed:0,total:1,bytesDownloaded:0,error:null});
 render(<I18nProvider><DownloadPanel embedded modelsDir="/configured" installTarget="/env" initialEntryId="curated" filenameFilter="wanted.safetensors"/></I18nProvider>);
 await screen.findByLabelText('wanted.safetensors');
 expect(screen.getByText('Fixture provider',{selector:'em.download-auth'})).not.toBeNull();
 expect(screen.queryByLabelText('other.pth')).toBeNull();
 expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_download_browse')).toHaveLength(1);
 expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_download_plan'||c==='native_download_start')).toBe(false);
 fireEvent.click(screen.getByLabelText('wanted.safetensors'));
 fireEvent.click(screen.getByRole('button',{name:'Download all (1)'}));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_download_start',{planId:'p',confirmed:true}));
});

it('targeted Library recovers running downloads after close and supports cancellation or completion',async()=>{
 let state='idle';
 const onStatus=vi.fn();
 const status=()=>({state,completed:state==='succeeded'?1:0,total:1,bytesDownloaded:0,error:null});
 vi.mocked(invoke).mockImplementation(async c=>{
  if(c==='native_download_catalog')return catalog;
  if(c==='native_auth_status')return [];
  if(c==='native_download_browse')return {resources:[resource],warnings:[],nextCursor:null};
  if(c==='native_files_present')return [false];
  if(c==='native_download_plan')return {id:'reopen',target:'/configured/models',resources:[resource],canExecute:true,warnings:[],totalKnownBytes:23,unknownSizeCount:0};
  if(c==='native_download_start')state='running';
  if(c==='native_download_cancel')state='cancelled';
  return status();
 });
 const panel=()=><I18nProvider><DownloadPanel embedded modelsDir="/configured" initialEntryId="curated" onStatus={onStatus}/></I18nProvider>;
 let view=render(panel());
 await screen.findByLabelText('Remote file');
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_download_status'));
 fireEvent.click(screen.getByLabelText('Remote file'));
 click('Download all (1)');
 await screen.findByText('Running');
 view.unmount();
 view=render(panel());
 await screen.findByText('Running');
 expect(screen.getByRole('button',{name:'Cancel download'})).toBeTruthy();
 click('Cancel download');
 await screen.findByText('Cancelled');
 view.unmount();
 state='succeeded';
 view=render(panel());
 await screen.findByText('Succeeded');
 expect(onStatus).toHaveBeenLastCalledWith(expect.objectContaining({state:'succeeded'}));
 expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_download_start')).toHaveLength(1);
 view.unmount();
});

it('Catalog source lists creator entries after the default DaSiWa catalog and browses the chosen creator live',async()=>{
 const creator={id:'civitai-creator-trashai',label:'Browse CivitAI creator TrashAI',providerId:'fixture',scope:'user',locator:'TrashAI',kind:'model',target:'other'};
 const withCreator={...catalog,providers:[{...catalog.providers[0],scopes:['index','repo','user']}],entries:[...catalog.entries,creator]};
 vi.mocked(invoke).mockImplementation(async c=>c==='native_download_catalog'?withCreator:c==='native_auth_status'?[]:c==='native_download_browse'?{resources:[resource],warnings:[],nextCursor:null}:c==='native_files_present'?[false]:null);
 await open();
 const source=screen.getByLabelText('Catalog source') as HTMLSelectElement;
 expect([...source.options].map(o=>o.textContent)).toEqual(['DaSiWa','TrashAI']);
 expect(source.value).toBe('');
 expect(screen.queryByRole('button',{name:/Browse CivitAI creator TrashAI/})).toBeNull();
 fireEvent.change(source,{target:{value:'civitai-creator-trashai'}});
 await screen.findByLabelText('Remote file');
 expect(invoke).toHaveBeenCalledWith('native_download_browse',expect.objectContaining({request:expect.objectContaining({providerId:'fixture',scope:'user',locator:'TrashAI'})}));
 expect(screen.queryByRole('button',{name:/Curated index/})).toBeNull();
 fireEvent.change(source,{target:{value:''}});
 expect(screen.getByRole('button',{name:/Curated index/})).toBeTruthy();
 expect(screen.queryByLabelText('Remote file')).toBeNull();
});
it('a creator source groups its models by category and base model like the bundled catalog',async()=>{
 const creator={id:'civitai-creator-trashai',label:'Browse CivitAI creator TrashAI',providerId:'fixture',scope:'user',locator:'TrashAI',kind:'model',target:'other'};
 const withCreator={...catalog,providers:[{...catalog.providers[0],scopes:['index','repo','user']}],entries:[...catalog.entries,creator]};
 const model=(id:string,label:string,category:string,base?:string)=>({...resource,id,label,kind:'repo',downloadable:false,locator:id,category,base});
 const listing=[model('1','Anima Checkpoint','checkpoints','Anima'),model('2','Illu LoRA','loras','Illustrious'),model('3','Anima LoRA','loras','Anima'),model('4','Some workflow','workflows')];
 vi.mocked(invoke).mockImplementation(async c=>c==='native_download_catalog'?withCreator:c==='native_auth_status'?[]:c==='native_download_browse'?{resources:listing,warnings:[],nextCursor:null}:c==='native_files_present'?[false]:null);
 await open();
 fireEvent.change(screen.getByLabelText('Catalog source'),{target:{value:'civitai-creator-trashai'}});
 await waitFor(()=>expect(document.querySelector('[data-source-categories]')).not.toBeNull());
 const tabs=document.querySelector<HTMLElement>('[data-source-categories]')!;
 expect([...tabs.querySelectorAll('[role="tab"]')].map(b=>b.textContent)).toEqual(['All','Models','LoRAs','Workflows']);
 const bases=()=>[...within(screen.getByRole('group',{name:'Base model'})).getAllByRole('button')].map(b=>b.textContent);
 expect(bases()).toEqual(['All','Anima','Illustrious']);
 const shown=()=>[...document.querySelectorAll('.download-resource .action-button')].map(e=>e.textContent?.replace(/^Select repository: /,'')).sort();
 expect(shown()).toEqual(['Anima Checkpoint','Anima LoRA','Illu LoRA','Some workflow']);
 fireEvent.click(within(tabs).getByRole('tab',{name:'LoRAs'}));
 expect(shown()).toEqual(['Anima LoRA','Illu LoRA']);
 expect(bases()).toEqual(['All','Anima','Illustrious']);
 fireEvent.click(within(screen.getByRole('group',{name:'Base model'})).getByRole('button',{name:'Anima'}));
 expect(shown()).toEqual(['Anima LoRA']);
});
it('uses exact nested IPC and metadata-only auth contracts',async()=>{
 const request={providerId:'fixture',scope:'index' as const,locator:'source',authenticated:false};await browseDownloads(request);expect(invoke).toHaveBeenLastCalledWith('native_download_browse',{request});
 const plan={providerId:'fixture',resources:[resource],parentPath:'/parent',folderName:'new',authenticated:false};await planDownloads(plan);expect(invoke).toHaveBeenLastCalledWith('native_download_plan',{request:plan});
 await setAuthToken('huggingface','private',false);expect(invoke).toHaveBeenLastCalledWith('native_auth_set',{providerId:'huggingface',token:'private',persist:false});await removeAuthToken('huggingface');expect(invoke).toHaveBeenLastCalledWith('native_auth_remove',{providerId:'huggingface'});
});
it('loads catalog only on explicit opening; browses curated sources and pagination explicitly',async()=>{
 mount();expect(invoke).not.toHaveBeenCalled();click('Library');await screen.findByRole('option',{name:'Curated index'});expect(vi.mocked(invoke).mock.calls.filter(c=>c[0]!=='native_auth_status')).toEqual([['native_download_catalog']]);
 fireEvent.change(screen.getByLabelText('Curated source'),{target:{value:'curated'}});click('Browse resources');await screen.findByLabelText('Remote file');expect(screen.getByText('Installed model')).toBeTruthy();expect(screen.queryByLabelText('Installed model')).toBeNull();click('Browse next page');await act(async()=>{});expect(invoke).toHaveBeenLastCalledWith('native_download_browse',{request:{providerId:'fixture',scope:'index',locator:'https://example.invalid/index',authenticated:false,nsfw:true,cursor:'page2'}});
});
it('plans and starts every selected file from one Download all click, locks edits and shows only real progress',async()=>{
 const start=screen.queryByRole('button',{name:'Download all (0)'});expect(start).toBeNull();
 await pick();expect((screen.getByRole('button',{name:'Download all (1)'}) as HTMLButtonElement).disabled).toBe(false);expect(within(screen.getByRole('region',{name:'Selected for download'})).getByText('Remote file')).toBeTruthy();
 click('Download all (1)');await screen.findByText('Running');expect(invoke).toHaveBeenCalledWith('native_download_plan',{request:{providerId:'fixture',resources:[resource],parentPath:'/parent',folderName:'new',authenticated:false}});expect(invoke).toHaveBeenCalledWith('native_download_start',{planId:'immutable',confirmed:true});expect((screen.getByLabelText('New subfolder') as HTMLInputElement).disabled).toBe(true);expect(screen.getByText('12')).toBeTruthy();click('Cancel download');await screen.findByText('Cancelled');
});
it('supports masked session-only tokens, clears on success and never displays native secret errors',async()=>{
 mount(true);click('Provider authorization');await act(async()=>{});fireEvent.click(screen.getByLabelText('Persist in OS credential store (optional)'));const input=screen.getByLabelText('Token') as HTMLInputElement;expect(input.type).toBe('password');fireEvent.change(input,{target:{value:'PRIVATE'}});click('Save token');await screen.findByText('Token check unavailable; try again');expect(invoke).toHaveBeenCalledWith('native_auth_set',{providerId:'huggingface',token:'PRIVATE',persist:false});expect(input.value).toBe('');expect(JSON.stringify(localStorage)).not.toContain('PRIVATE');vi.mocked(invoke).mockRejectedValueOnce('PRIVATE');fireEvent.change(input,{target:{value:'PRIVATE'}});click('Save token');await screen.findByRole('alert');expect(document.body.textContent).not.toContain('PRIVATE');click('Remove token');await act(async()=>{});expect(invoke).toHaveBeenCalledWith('native_auth_remove',{providerId:'huggingface'});
});
it('discards stale browse results after an edit and serializes duplicate actions',async()=>{
 await open();let resolve!:(value:unknown)=>void;
 vi.mocked(invoke).mockImplementationOnce(()=>new Promise(r=>{resolve=r;}) as never);
 fireEvent.change(screen.getByLabelText('Source locator'),{target:{value:'first'}});click('Browse resources');click('Browse resources');
 fireEvent.change(screen.getByLabelText('Source locator'),{target:{value:'second'}});
 await act(async()=>resolve([]));
 expect(screen.queryByLabelText('Remote file')).toBeNull();expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_download_browse')).toHaveLength(1);
});
it('refreshes auth metadata only, makes persistence explicit and clears token on unmount',async()=>{
 const view=mount(true);click('Provider authorization');click('Refresh authorization status');await act(async()=>{});expect(invoke).toHaveBeenCalledWith('native_auth_status');
 const input=screen.getByLabelText('Token') as HTMLInputElement;fireEvent.change(input,{target:{value:'PRIVATE'}});expect((screen.getByLabelText('Persist in OS credential store (optional)') as HTMLInputElement).checked).toBe(true);click('Save token');await act(async()=>{});expect(invoke).toHaveBeenCalledWith('native_auth_set',{providerId:'huggingface',token:'PRIVATE',persist:true});
 fireEvent.change(input,{target:{value:'PRIVATE'}});view.unmount();expect(input.value).toBe('');
});
it('cancels polling timers and ignores an in-flight poll after unmount',async()=>{
 await pick();click('Download all (1)');await screen.findByText('Running');
 vi.useFakeTimers();let resolve!:(value:unknown)=>void;vi.mocked(invoke).mockImplementationOnce(()=>new Promise(r=>{resolve=r;}) as never);
 // Explicit status refresh exercises the same lifecycle guard as timed polling.
 click('Refresh download status');cleanup();await act(async()=>resolve({state:'succeeded',completed:1,total:1,bytesDownloaded:23,error:null}));await act(async()=>vi.advanceTimersByTimeAsync(5000));
 expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_download_status')).toHaveLength(1);
});
it('locks meaning edits while native start is unresolved',async()=>{
 await pick();let resolve!:(value:unknown)=>void;vi.mocked(invoke).mockImplementationOnce(()=>new Promise(r=>{resolve=r;}) as never);
 click('Download all (1)');
 expect((screen.getByLabelText('New subfolder') as HTMLInputElement).disabled).toBe(true);
 await act(async()=>resolve({id:'immutable',target:'/parent/new',resources:[resource],totalKnownBytes:23,unknownSizeCount:0,warnings:[],canExecute:true}));await screen.findByText('Running');
});
it('turns user repository listings into explicit repository browsing',async()=>{
 await open();vi.mocked(invoke).mockResolvedValueOnce([]).mockResolvedValueOnce({resources:[{id:'repo',label:'Owner/repo',kind:'repo',downloadable:false,locator:'models:Owner/repo'}],nextCursor:null,warnings:[]});fireEvent.change(screen.getByLabelText('Source locator'),{target:{value:'Owner'}});click('Browse resources');await screen.findByRole('button',{name:'Select repository: Owner/repo'});click('Select repository: Owner/repo');await act(async()=>{});expect((screen.getByLabelText('Source locator') as HTMLInputElement).value).toBe('models:Owner/repo');expect((screen.getByLabelText('Browse mode') as HTMLSelectElement).value).toBe('repo');expect(invoke).toHaveBeenLastCalledWith('native_download_browse',{request:{providerId:'fixture',scope:'repo',locator:'models:Owner/repo',authenticated:false,nsfw:true}});
});
it('imports data-only catalogs explicitly and adopts returned provider entries without browsing',async()=>{
 await open();const text='version = 1\nentries = []';
 vi.mocked(invoke).mockResolvedValueOnce({...catalog,entries:[...catalog.entries,{...catalog.entries[0],id:'extra',label:'Added entry'}]});
 fireEvent.change(screen.getByLabelText('TOML catalog data'),{target:{value:text}});click('Validate and extend catalog');await screen.findByRole('option',{name:'Added entry'});
 expect(invoke).toHaveBeenLastCalledWith('native_download_import_catalog',{text});expect((screen.getByLabelText('TOML catalog data') as HTMLTextAreaElement).value).toBe('');expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_download_browse')).toBe(false);
});
it('loads metadata automatically and retains explicit restore for retries',async()=>{
 mount(true);click('Provider authorization');await act(async()=>{});expect(invoke).toHaveBeenCalledWith('native_auth_status');vi.mocked(invoke).mockResolvedValueOnce({providerId:'huggingface',configured:true,persisted:true});click('Explicitly restore saved token');await screen.findByText(/Token check unavailable; try again/);expect(invoke).toHaveBeenCalledWith('native_auth_restore',{providerId:'huggingface'});expect((screen.getByLabelText('Token') as HTMLInputElement).value).toBe('');
});
it('shows the login hint only for providers without a saved token',async()=>{
 vi.mocked(invoke).mockImplementation(async c=>c==='native_auth_status'?[]:c==='native_download_catalog'?{...catalog,entries:[{id:'g',label:'Gated',providerId:'fixture',scope:'index',locator:'https://example.invalid/g.json',kind:'model',target:'loras',auth:true}]}:null);
 render(<I18nProvider><DownloadPanel embedded/></I18nProvider>);expect(await screen.findByText('Login / API key needed')).toBeTruthy();
});
it('labels unavailable browser transport without IPC',()=>{vi.mocked(isTauri).mockReturnValue(false);mount();click('Library');expect(screen.getByText(/Browser preview/)).toBeTruthy();expect(invoke).not.toHaveBeenCalled();});
it('shows catalog categories as cards with previews, opens an entry on click and targets the managed ComfyUI folder',async()=>{
 const entries=[{id:'lora-a',label:'Lora A',providerId:'fixture',scope:'index',locator:'https://example.invalid/a.json',kind:'model',target:'loras',preview:'https://image.civitai.com/a.jpeg'},{id:'wf',label:'Workflow W',providerId:'fixture',scope:'index',locator:'https://example.invalid/w.json',kind:'workflow',target:'workflows'},{id:'ckpt',label:'Image checkpoint',providerId:'fixture',scope:'index',locator:'https://example.invalid/c.json',kind:'model',target:'checkpoints',media:'image',auth:true},{id:'wan',label:'Video model',providerId:'fixture',scope:'index',locator:'https://example.invalid/v.json',kind:'model',target:'diffusion_models',media:'video'}];
 vi.mocked(invoke).mockImplementation(async(c,a)=>c==='native_auth_status'?[{providerId:'fixture',configured:true,persisted:true},{providerId:'civitai',configured:true,persisted:true}]:c==='native_download_catalog'?{...catalog,providers:[...catalog.providers,{id:'civitai',kind:'civitai',label:'CivitAI',origin:'https://civitai.com',scopes:['user','model','search'],anonymousNotice:''}],entries}:c==='native_download_browse'?((a as {request:{scope:string}}).request.scope==='search'?{resources:[{id:'civitai:7',label:'Found · v1',kind:'repo',downloadable:false,locator:'7',category:'checkpoints',preview:'https://image.civitai.com/f.jpeg'}],nextCursor:null,warnings:[]}:{resources:[{...resource,preview:'https://image.civitai.com/r.jpeg'}],nextCursor:null,warnings:[]}):c==='native_download_plan'?{id:'p',target:(a as {request:{parentPath:string}}).request.parentPath,resources:[resource],totalKnownBytes:23,unknownSizeCount:0,warnings:[],canExecute:true}:{state:'idle',completed:0,total:0,bytesDownloaded:0,error:null});
 render(<I18nProvider><DownloadPanel installTarget="/env"/></I18nProvider>);click('Library');
 await screen.findByRole('tab',{name:'Models'});const tabs=within(screen.getByRole('tablist',{name:'Category'})).getAllByRole('tab');expect(tabs.map(t=>t.textContent)).toEqual(['Models','LoRAs','Workflows']);expect(tabs[0].getAttribute('aria-selected')).toBe('true');
 expect(screen.getByRole('button',{name:/Image checkpoint/})).toBeTruthy();await act(async()=>{});expect(screen.queryByText('Login / API key needed')).toBeNull();fireEvent.click(screen.getByRole('button',{name:'Video'}));expect(screen.queryByRole('button',{name:/Image checkpoint/})).toBeNull();expect(screen.getByRole('button',{name:/Video model/})).toBeTruthy();fireEvent.click(screen.getByRole('button',{name:'All'}));
 fireEvent.click(screen.getByRole('button',{name:/Image checkpoint/}));await screen.findByText('/env/ComfyUI/models/checkpoints');await screen.findByLabelText('Remote file');expect((screen.getByLabelText('Access mode') as HTMLSelectElement).value).toBe('true');
 fireEvent.click(screen.getByRole('tab',{name:'LoRAs'}));
 expect(document.querySelector('.download-card img')).toBeTruthy();expect(screen.queryByRole('button',{name:'Workflow W'})).toBeNull();expect(screen.queryByRole('button',{name:/Image checkpoint/})).toBeNull();
 fireEvent.click(screen.getByRole('tab',{name:'Workflows'}));expect(screen.getByRole('button',{name:'Workflow W'})).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Workflow W'}));await screen.findByLabelText('Remote file');
 expect(invoke).toHaveBeenLastCalledWith('native_download_browse',{request:{providerId:'fixture',scope:'index',locator:'https://example.invalid/w.json',authenticated:true,nsfw:true}});
 expect(document.querySelector('.download-resource img')).toBeTruthy();expect(screen.getByText('/env/ComfyUI/user/default/workflows')).toBeTruthy();expect(screen.queryByRole('button',{name:'Choose existing parent folder'})).toBeNull();
 fireEvent.click(screen.getByLabelText('Remote file'));click('Download all (1)');await act(async()=>{});
 expect(invoke).toHaveBeenCalledWith('native_download_plan',{request:{providerId:'fixture',resources:[expect.objectContaining({id:'file'})],parentPath:'/env/ComfyUI/user/default/workflows',folderName:'',authenticated:true}});
 fireEvent.change(screen.getByLabelText('Search catalog & CivitAI'),{target:{value:'found'}});click('Search');await screen.findByRole('button',{name:'Select repository: Found · v1'});
 expect(invoke).toHaveBeenLastCalledWith('native_download_browse',{request:{providerId:'civitai',scope:'search',locator:'found',authenticated:true,nsfw:true}});
 click('Select repository: Found · v1');await screen.findByLabelText('Remote file');expect(screen.getByText('/env/ComfyUI/models/checkpoints')).toBeTruthy();
 expect(invoke).toHaveBeenLastCalledWith('native_download_browse',{request:{providerId:'civitai',scope:'model',locator:'7',authenticated:true,nsfw:true}});
});
it('mounts the Library only on its top tab; authorization lives in Backend panels',async()=>{
 const view=render(<I18nProvider><DownloadWorkspace view={null} authRevision={0}/></I18nProvider>);expect(screen.queryByLabelText('Token')).toBeNull();expect(screen.queryByRole('tab')).toBeNull();
 view.rerender(<I18nProvider><DownloadWorkspace view="downloads" authRevision={0}/></I18nProvider>);await screen.findAllByRole('tab');expect(screen.queryByLabelText('Token')).toBeNull();
 view.rerender(<I18nProvider><DownloadWorkspace view={null} authRevision={0}/></I18nProvider>);expect(screen.queryByRole('tab')).toBeNull();
});
it('always lists adult entries unblurred with their 18+ tag and browses everything natively',async()=>{
 const entries=[{id:'sfw',label:'Safe model',providerId:'fixture',scope:'index',locator:'https://example.invalid/s.json',kind:'model',target:'checkpoints',preview:'https://example.invalid/s.png'},{id:'adult',label:'Adult model',providerId:'fixture',scope:'index',locator:'https://example.invalid/a.json',kind:'model',target:'checkpoints',nsfw:true,preview:'https://example.invalid/a.png'}];
 vi.mocked(invoke).mockImplementation(async c=>c==='native_auth_status'?[]:c==='native_download_catalog'?{...catalog,entries}:{resources:[{...resource,nsfw:true}],nextCursor:null,warnings:[]});
 mount();click('Library');const adult=await screen.findByRole('button',{name:/Adult model/});
 expect(screen.queryByRole('switch')).toBeNull();expect(document.querySelector('.download-preview.is-blurred')).toBeNull();expect(within(adult).getByText('18+')).toBeTruthy();
 fireEvent.click(adult);await screen.findByLabelText('Remote file');
 expect(invoke).toHaveBeenLastCalledWith('native_download_browse',{request:{providerId:'fixture',scope:'index',locator:'https://example.invalid/a.json',authenticated:false,nsfw:true}});
});
it('keeps selections across cards and downloads one native plan per folder in sequence',async()=>{
 const entries=[{...catalog.entries[0],id:'lora',label:'Lora card',locator:'https://example.invalid/l.json',target:'loras'},{...catalog.entries[0],id:'wf',label:'Workflow card',locator:'https://example.invalid/w.json',target:'loras'}];
 const plans:string[]=[];
 vi.mocked(invoke).mockImplementation(async(c,a)=>{const r=(a as {request:{locator:string;parentPath:string}}|undefined)?.request;
  if(c==='native_download_catalog')return {...catalog,entries:[entries[0],{...entries[1],target:'workflows'}]};if(c==='native_auth_status')return [];
  if(c==='native_download_browse')return {resources:[{...resource,id:r!.locator,label:`File ${r!.locator.slice(-6)}`}],nextCursor:null,warnings:[]};
  if(c==='native_download_plan'){plans.push(r!.parentPath);return {id:`p${plans.length}`,target:r!.parentPath,resources:[],totalKnownBytes:0,unknownSizeCount:0,warnings:[],canExecute:true};}
  return {state:'succeeded',completed:1,total:1,bytesDownloaded:1,error:null};});
 render(<I18nProvider><DownloadPanel installTarget="/env"/></I18nProvider>);click('Library');
 fireEvent.click(await screen.findByRole('button',{name:'Lora card'}));fireEvent.click(await screen.findByLabelText('File l.json'));
 tab('Workflows');fireEvent.click(await screen.findByRole('button',{name:'Workflow card'}));fireEvent.click(await screen.findByLabelText('File w.json'));
 const basket=screen.getByRole('region',{name:'Selected for download'});expect(within(basket).getByText('loras')).toBeTruthy();expect(within(basket).getByText('workflows')).toBeTruthy();
 click('Download all (2)');await waitFor(()=>expect(within(screen.getByRole('region',{name:'Selected for download'})).queryAllByRole('listitem')).toHaveLength(0));
 expect(plans).toEqual(['/env/ComfyUI/models/loras','/env/ComfyUI/user/default/workflows']);expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_download_start').map(([,a])=>a)).toEqual([{planId:'p1',confirmed:true},{planId:'p2',confirmed:true}]);
});
it('narrows catalog cards live by the search text',async()=>{
 const entries=[{...catalog.entries[0],id:'a',label:'Wan Lightspeed'},{...catalog.entries[0],id:'b',label:'LTX Director'}];
 vi.mocked(invoke).mockImplementation(async c=>c==='native_auth_status'?[]:c==='native_download_catalog'?{...catalog,entries}:null);
 mount();click('Library');await screen.findByRole('button',{name:'LTX Director'});
 fireEvent.change(screen.getByLabelText('Search catalog & CivitAI'),{target:{value:'light'}});
 expect(screen.getByRole('button',{name:'Wan Lightspeed'})).toBeTruthy();expect(screen.queryByRole('button',{name:'LTX Director'})).toBeNull();
 expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_download_browse')).toBe(false);
});
it('filters catalog cards and listed files by base model family',async()=>{
 const entries=[{id:'wan',label:'Wan model',providerId:'fixture',scope:'index',locator:'https://example.invalid/w.json',kind:'model',target:'checkpoints',base:'Wan 2.2'},{id:'ltx',label:'LTX model',providerId:'fixture',scope:'index',locator:'https://example.invalid/l.json',kind:'model',target:'checkpoints',base:'LTX 2.3'}];
 vi.mocked(invoke).mockImplementation(async c=>c==='native_auth_status'?[]:c==='native_download_catalog'?{...catalog,entries}:{resources:[{...resource,base:'Wan 2.2'},{...resource,id:'l',label:'LTX file',base:'LTX 2.3'}],nextCursor:null,warnings:[]});
 mount();click('Library');await screen.findByRole('button',{name:/Wan model/});
 const group=screen.getByRole('group',{name:'Base model'});expect([...group.querySelectorAll('button')].map(b=>b.textContent)).toEqual(['All','LTX 2.3','Wan 2.2']);
 fireEvent.click(screen.getByRole('button',{name:'LTX 2.3'}));expect(screen.queryByRole('button',{name:/Wan model/})).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:/LTX model/}));await screen.findByLabelText('LTX file');expect(screen.queryByLabelText('Remote file')).toBeNull();
});
it('shows every provider as a button with its saved-token state and opens the clicked one',async()=>{
 vi.mocked(invoke).mockImplementation(async c=>c==='native_auth_status'?[{providerId:'civitai',configured:true,persisted:true}]:c==='native_auth_verify'?{providerId:'civitai',status:'verified'}:null);
 mount(true);click('Provider authorization');
 const group=await screen.findByRole('group',{name:'Provider'});await screen.findByRole('button',{name:'CivitAI Token saved'});
 expect([...group.querySelectorAll('button')].map(b=>[b.textContent,b.getAttribute('aria-pressed')])).toEqual([['Hugging FaceNo token','true'],['✓ CivitAIToken saved','false'],['GitHubNo token','false']]);
 fireEvent.click(screen.getByRole('button',{name:'CivitAI Token saved'}));expect(screen.getByRole('button',{name:'CivitAI Token saved'}).getAttribute('aria-pressed')).toBe('true');expect(screen.getByText('https://civitai.com',{exact:false})).toBeTruthy();
});
it('opens the fixed provider token page natively without sending any URL',async()=>{
 mount(true);click('Provider authorization');fireEvent.click(screen.getByRole('button',{name:/^CivitAI/}));click('Open token page in browser');await act(async()=>{});
 expect(invoke).toHaveBeenCalledWith('native_open_token_page',{providerId:'civitai'});
});
it('sorts catalog cards and files newest first, then by name, with missing dates last',async()=>{
 const entries=[{...catalog.entries[0],id:'old',label:'Alpha',publishedAt:'2025-01-01T00:00:00Z'},{...catalog.entries[0],id:'new',label:'Zulu',publishedAt:'2026-01-01T00:00:00Z'},{...catalog.entries[0],id:'unknown',label:'Beta',publishedAt:undefined}];
 vi.mocked(invoke).mockImplementation(async c=>c==='native_auth_status'?[]:c==='native_download_catalog'?{...catalog,entries}:{resources:entries.map(e=>({...resource,id:e.id,label:e.label,publishedAt:e.publishedAt})),nextCursor:null,warnings:[]});
 mount();click('Library');await screen.findByRole('button',{name:'Zulu'});
 const cards=()=>[...document.querySelectorAll('.download-card')].map(e=>e.querySelector('span:last-of-type')!.textContent);
 expect(cards()).toEqual(['Zulu','Alpha','Beta']);
 click('Zulu');await screen.findByLabelText('Zulu');
 const rows=()=>[...document.querySelectorAll('.download-resource label')].map(e=>e.textContent);
 expect(rows()).toEqual(['Zulu','Alpha','Beta']);
 const sort=screen.getByRole('group',{name:'Sort by'});const sortButton=(name:string)=>[...sort.querySelectorAll('button')].find(b=>b.textContent?.startsWith(name))!;
 expect(sortButton('Newest').getAttribute('aria-pressed')).toBe('true');expect(sortButton('Newest').textContent).toBe('Newest ↓');
 fireEvent.click(sortButton('Newest'));expect(sortButton('Newest').textContent).toBe('Newest ↑');expect(cards()).toEqual(['Alpha','Zulu','Beta']);
 fireEvent.click(sortButton('Name'));expect(sortButton('Name').getAttribute('aria-pressed')).toBe('true');expect(sortButton('Newest').getAttribute('aria-pressed')).toBe('false');
 expect(cards()).toEqual(['Alpha','Beta','Zulu']);expect(rows()).toEqual(['Alpha','Beta','Zulu']);
 fireEvent.click(sortButton('Name'));expect(sortButton('Name').textContent).toBe('Name ↓');expect(cards()).toEqual(['Zulu','Beta','Alpha']);
 expect((screen.getByLabelText('Zulu') as HTMLInputElement).checked).toBe(false);
});
it('shows loading and a browse failure inside the clicked card picker',async()=>{
 let fail!:(e:unknown)=>void;vi.mocked(invoke).mockImplementation(async c=>c==='native_download_catalog'?catalog:c==='native_auth_status'?[]:c==='native_download_browse'?new Promise((_,reject)=>{fail=reject;}):{state:'idle',completed:0,total:0,bytesDownloaded:0,error:null});
 mount();click('Library');fireEvent.click(await screen.findByRole('button',{name:'Curated index'}));await waitFor(()=>expect(fail).toBeTypeOf('function'));
 expect(document.querySelector('.download-cards .download-picker [role="status"]')).toBeTruthy();
 await act(async()=>fail('Unsupported CivitAI file selector query'));
 expect(document.querySelector('.download-cards .download-picker [role="alert"]')?.textContent).toMatch(/Could not load the files/);expect(screen.getAllByRole('alert')).toHaveLength(1);
});
it('opens the file picker under the clicked card with one version selected at a time',async()=>{
 vi.mocked(invoke).mockImplementation(async c=>c==='native_auth_status'?[]:c==='native_download_catalog'?catalog:{resources:[{...resource,id:'a',label:'model-fp8.safetensors',variant:'fp8 · pruned · ≈13.5 GB',version:'v2',publishedAt:'2026-02-01T00:00:00Z'},{...resource,id:'b',label:'model-fp16.safetensors',version:'v2',publishedAt:'2026-02-01T00:00:00Z'},{...resource,id:'c',label:'model-old.safetensors',version:'v1',publishedAt:'2025-01-01T00:00:00Z'}],nextCursor:null,warnings:[]});
 mount();click('Library');fireEvent.click(await screen.findByRole('button',{name:'Curated index'}));await screen.findByLabelText(/model-fp8\.safetensors/);expect(screen.getByText('fp8 · pruned · ≈13.5 GB')).toBeTruthy();
 expect(document.querySelector('.download-cards .download-picker')).toBeTruthy();
 const group=screen.getByRole('group',{name:'Version'});expect([...group.querySelectorAll('button')].map(b=>b.textContent)).toEqual(['v2','v1']);
 expect(screen.getByRole('button',{name:'v2'}).getAttribute('aria-pressed')).toBe('true');expect(screen.queryByLabelText('model-old.safetensors')).toBeNull();
 fireEvent.click(screen.getByLabelText(/model-fp8\.safetensors/));fireEvent.click(screen.getByRole('button',{name:'v1'}));
 expect(screen.getByLabelText('model-old.safetensors')).toBeTruthy();expect(screen.queryByRole('checkbox',{name:/model-fp8\.safetensors/})).toBeNull();expect(within(screen.getByRole('region',{name:'Selected for download'})).getByText('model-fp8.safetensors')).toBeTruthy();
});
it('marks files that already exist in the destination as local and keeps them unselectable',async()=>{
 vi.mocked(invoke).mockImplementation(async c=>c==='native_auth_status'?[]:c==='native_download_catalog'?catalog:c==='native_download_browse'?{resources:[{...resource,locator:'have.safetensors'},{...resource,id:'new',label:'New file',locator:'new.safetensors'}],nextCursor:null,warnings:[]}:c==='native_files_present'?[true,false]:{state:'idle',completed:0,total:0,bytesDownloaded:0,error:null});
 render(<I18nProvider><DownloadPanel installTarget="/env" modelsDir="/models"/></I18nProvider>);click('Library');
 fireEvent.click(await screen.findByRole('button',{name:'Curated index'}));await screen.findByLabelText('New file');
 await screen.findByText(/already local/);expect(screen.queryByLabelText('Remote file')).toBeNull();
 expect(invoke).toHaveBeenCalledWith('native_files_present',{paths:['/models/models/have.safetensors','/models/models/new.safetensors']});
});

it('shows already restored authorization on open without returning a token',async()=>{
 vi.mocked(invoke).mockResolvedValueOnce([{providerId:'huggingface',configured:true,persisted:true}]);
 mount(true);click('Provider authorization');await screen.findByText(/Token check unavailable; try again/);
 expect(invoke).toHaveBeenCalledWith('native_auth_status');expect((screen.getByLabelText('Token') as HTMLInputElement).value).toBe('');
 expect((screen.getByLabelText('Persist in OS credential store (optional)') as HTMLInputElement).checked).toBe(true);
});
it('automatically uses the matching configured token for browse and plan but respects anonymous choice',async()=>{
 await open();const original=vi.mocked(invoke).getMockImplementation()!;
 vi.mocked(invoke).mockImplementation((c,a)=>c==='native_auth_status'?Promise.resolve([{providerId:'fixture',configured:true,persisted:true}]):original(c,a));
 fireEvent.change(screen.getByLabelText('Source locator'),{target:{value:'source'}});click('Browse resources');await screen.findByLabelText('Remote file');
 expect(invoke).toHaveBeenLastCalledWith('native_download_browse',{request:{providerId:'fixture',scope:'index',locator:'source',authenticated:true,nsfw:true}});
 fireEvent.click(screen.getByLabelText('Remote file'));click('Choose existing parent folder');await screen.findByText('/parent');
 fireEvent.change(screen.getByLabelText('Access mode'),{target:{value:'false'}});click('Browse resources');await screen.findByLabelText('Remote file');
 expect(invoke).toHaveBeenLastCalledWith('native_download_browse',{request:expect.objectContaining({authenticated:false})});
 // The ticked file keeps the access mode it was listed with.
 click('Download all (1)');await screen.findByText('Running');expect(invoke).toHaveBeenCalledWith('native_download_plan',{request:expect.objectContaining({authenticated:true})});
});
it('returns from Installed VAE to a valid catalog tab and preserves valid remembered choices',async()=>{
 const entries=[{...catalog.entries[0],target:'checkpoints'},{...catalog.entries[0],id:'lora',label:'Catalog LoRA',target:'loras'}];
 vi.mocked(invoke).mockImplementation(async c=>c==='native_download_catalog'?{...catalog,entries}:c==='native_models_scan'?[{category:'vae',name:'v.pt',path:'/env/ComfyUI/models/vae/v.pt',size:3,info:null},{category:'loras',name:'a.pt',path:'/env/ComfyUI/models/loras/a.pt',size:3,info:null}]:[]);
 localStorage.setItem('deas.pref.library.category',JSON.stringify('loras'));
 render(<I18nProvider><DownloadPanel embedded installTarget="/env"/></I18nProvider>);
 expect((await screen.findByRole('tab',{name:'LoRAs'})).getAttribute('aria-selected')).toBe('true');
 tab('Installed');fireEvent.click(await screen.findByRole('tab',{name:'VAE'}));tab('Catalog');
 expect(screen.getByRole('tab',{name:'Models'}).getAttribute('aria-selected')).toBe('true');
 expect(screen.getByRole('button',{name:'Curated index'})).toBeTruthy();
 fireEvent.click(screen.getByRole('tab',{name:'LoRAs'}));tab('Installed');await screen.findByRole('button',{name:'a.pt'});tab('Catalog');
 expect(screen.getByRole('tab',{name:'LoRAs'}).getAttribute('aria-selected')).toBe('true');
 expect(screen.getByRole('button',{name:'Catalog LoRA'})).toBeTruthy();
});
it('offers Edit meta on installed cards without fetching or downloading and disables companion family editing',async()=>{
 Object.defineProperty(HTMLDialogElement.prototype,'showModal',{configurable:true,value:function(){this.open=true;}});
 const model={category:'vae',name:'companion.safetensors',path:'/models/vae/companion.safetensors',size:1,info:null};
 vi.mocked(invoke).mockImplementation(async c=>c==='native_download_catalog'?catalog:c==='native_models_scan'?[model]:c==='native_model_scan_status'?{state:'idle',id:''}:[]);
 render(<I18nProvider><DownloadPanel embedded installTarget="/env" modelsDir="/models"/></I18nProvider>);await screen.findByRole('tab',{name:'Models'});tab('Installed');
 fireEvent.contextMenu(await screen.findByRole('button',{name:model.name}));fireEvent.click(screen.getByRole('menuitem',{name:'Edit meta…'}));
 expect(screen.getByRole('dialog').textContent).toContain(model.path);expect(within(screen.getByRole('dialog')).getByLabelText('Model family')).toHaveProperty('disabled',true);
 expect(screen.getByText('Family editing is unsupported for this model category.')).toBeTruthy();
 expect(vi.mocked(invoke).mock.calls.some(([c])=>['native_model_info','native_download_start','native_model_family_write'].includes(c))).toBe(false);
});
it('saves and resets installed family metadata locally, showing declaration on card and details',async()=>{
 Object.defineProperty(HTMLDialogElement.prototype,'showModal',{configurable:true,value:function(){this.open=true;}});
 const model={category:'checkpoints',name:'base.safetensors',path:'/models/checkpoints/base.safetensors',size:1,info:null,detectedFamily:'sdxl',family:'sdxl'};
 vi.mocked(invoke).mockImplementation(async(c,a)=>c==='native_download_catalog'?catalog:c==='native_models_scan'?[model]:c==='native_model_scan_status'?{state:'idle',id:''}:c==='native_model_family_write'?{...model,...((a as {family:string|null}).family?{familyOverride:'sdxl'}:{})}:[]);
 render(<I18nProvider><DownloadPanel embedded installTarget="/env" modelsDir="/models"/></I18nProvider>);await screen.findByRole('tab',{name:'Models'});tab('Installed');
 const card=await screen.findByRole('button',{name:model.name});fireEvent.click(card);fireEvent.keyDown(card,{key:'ContextMenu'});fireEvent.click(screen.getByRole('menuitem',{name:'Edit meta…'}));
 expect(within(screen.getByRole('dialog')).getByLabelText('Model family')).toHaveProperty('value','sdxl');fireEvent.click(screen.getByRole('button',{name:'Save'}));await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());
 expect(invoke).toHaveBeenCalledWith('native_model_family_write',{path:model.path,family:'sdxl'});
 expect(card.textContent).toContain('User-declared family');expect(document.querySelector('.download-picker .model-declared')).toBeTruthy();
 fireEvent.contextMenu(card);fireEvent.click(screen.getByRole('menuitem',{name:'Edit meta…'}));fireEvent.click(screen.getByRole('button',{name:'Reset to detection'}));await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());
 expect(invoke).toHaveBeenCalledWith('native_model_family_write',{path:model.path,family:null});expect(document.querySelector('.model-declared')).toBeNull();
 expect(vi.mocked(invoke).mock.calls.some(([c])=>['native_model_info','native_download_start','native_model_scan_start'].includes(c))).toBe(false);
 // A save from another mounted view refreshes this exact installed path without rescanning.
 act(()=>window.dispatchEvent(new CustomEvent('deas:model-family-changed',{detail:{...model,familyOverride:'sdxl'}})));
 expect(card.textContent).toContain('User-declared family');expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_models_scan')).toHaveLength(1);
});
it('lists installed models per folder and fetches CivitAI metadata by hash on request',async()=>{
 const local={category:'loras',name:'Krea2/slime.safetensors',path:'/env/ComfyUI/models/loras/Krea2/slime.safetensors',size:2147483648,info:null};
 const fetched={...local,info:{sha256:'ab12',size:2147483648,fetchedAt:1,civitai:{modelId:1,versionId:2,name:'True Slime Girls',version:'Krea2 v1.0',modelType:'LORA',category:'loras',base:'Krea 2',nsfw:false,trainedWords:['D4S1W4_SL1ME'],preview:'https://image.civitai.com/x/width=450/1.jpeg',settings:{sampler:'Euler a',steps:30,cfgScale:4.5,size:'896x1152'}}}};
 vi.mocked(invoke).mockImplementation(async(c,a)=>{
  if(c==='native_download_catalog')return catalog;
  if(c==='native_models_scan'){expect(a).toEqual({root:'/env/ComfyUI/models'});return [local,{...local,category:'vae',name:'v.pt',path:'/env/ComfyUI/models/vae/v.pt'}];}
  if(c==='native_model_info'){const p=(a as {path:string}).path;expect(a).toEqual({path:p,authenticated:false});return p===local.path?fetched:{...local,category:'vae',name:'v.pt',path:'/env/ComfyUI/models/vae/v.pt',info:{sha256:'cd34',size:2097152,fetchedAt:1,civitai:null}};}
  if(c==='native_model_scan_status')return {id:'',root:'',state:'idle',file:null,completed:0,total:0,remaining:0,failed:0};
  if(c==='native_model_scan_start')return {id:'scan',root:'/env/ComfyUI/models',state:'running',file:local.path,completed:0,total:2,remaining:2,failed:0};
  if(c==='native_auth_status')return [{providerId:'civitai',configured:false,persisted:false}];
  throw new Error(`unexpected ${c}`);
 });
 render(<I18nProvider><DownloadPanel embedded installTarget="/env"/></I18nProvider>);
 await screen.findByRole('tab',{name:'Models'});
 expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_models_scan')).toBe(false); // scanned only when the installed view opens
 tab('Installed');
 expect((await screen.findByRole('tab',{name:'LoRAs'})).getAttribute('aria-selected')).toBe('true');
 expect(screen.getByRole('tab',{name:'VAE'})).toBeTruthy();
 fireEvent.click(await screen.findByRole('button',{name:'Krea2/slime.safetensors'}));
 expect(screen.getByText('not hashed yet')).toBeTruthy();
 click('Fetch CivitAI metadata');
 expect(await screen.findByText('D4S1W4_SL1ME')).toBeTruthy();
 expect(screen.getByText('True Slime Girls · Krea2 v1.0 · Krea 2')).toBeTruthy();
 expect(screen.getByText('Euler a · 30 steps · CFG 4.5 · 896x1152')).toBeTruthy();
 expect(screen.getByRole('button',{name:'Refresh metadata'})).toBeTruthy();
 expect(document.querySelector('img.download-preview')?.getAttribute('src')).toBe(fetched.info.civitai.preview);
 // Batch scan: scope selection + settings provenance hint, then one native background job.
 expect(screen.getByLabelText('Scope')).toBeTruthy();
 expect(screen.getByText(/Example settings are not verified recommendations/)).toBeTruthy();
 click('Scan Files');
 // One native job owns the sequential batch; React never drives per-file requests.
 click('Scan Files');
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_model_scan_start',{root:'/env/ComfyUI/models',scope:'all',authenticated:false}));
 expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_model_scan_start')).toHaveLength(1);
 expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_model_info')).toHaveLength(1);
 expect((screen.getByRole('button',{name:'Scan Files'}) as HTMLButtonElement).disabled).toBe(true);
});
it('reattaches to the native scan after tab changes and unmount without restarting it',async()=>{
 const local={category:'loras',name:'scan.pt',path:'/models/loras/scan.pt',size:5,info:null};
 let status={id:'background',root:'/models',state:'running',file:local.path,completed:1,total:3,remaining:2,failed:0};
 vi.mocked(invoke).mockImplementation(async c=>c==='native_download_catalog'?catalog:c==='native_auth_status'?[]:c==='native_models_scan'?[local]:c==='native_model_scan_status'?status:null);
 const mountLibrary=()=>render(<I18nProvider><DownloadPanel embedded installTarget="/env" modelsDir="/models"/></I18nProvider>);
 const view=mountLibrary();await screen.findByRole('tab',{name:'Models'});tab('Installed');
 await screen.findByText(/1\/3 processed · 2 remaining · 0 failed/);
 expect((screen.getByRole('button',{name:'Scan Files'}) as HTMLButtonElement).disabled).toBe(true);
 tab('Catalog');status={...status,completed:2,remaining:1};tab('Installed');
 await screen.findByText(/2\/3 processed · 1 remaining · 0 failed/);
 view.unmount();mountLibrary();await screen.findByRole('tab',{name:'Models'});tab('Installed');
 await screen.findByText(/2\/3 processed · 1 remaining · 0 failed/);
 status={...status,state:'failed',completed:3,remaining:0,failed:1};
 await waitFor(()=>expect(screen.getByText(/Scan finished with errors.*3\/3 processed · 0 remaining · 1 failed/)).toBeTruthy(),{timeout:2500});
 expect((screen.getByRole('button',{name:'Scan Files'}) as HTMLButtonElement).disabled).toBe(false);
 expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_model_scan_start'||c==='native_model_info'||c==='native_download_cancel')).toBe(false);
});
it('does not update or continue polling an unmounted scan view',async()=>{
 vi.useFakeTimers();let resolve!:(value:unknown)=>void;
 vi.mocked(invoke).mockImplementation(async c=>c==='native_download_catalog'?catalog:c==='native_auth_status'?[]:c==='native_models_scan'?[{category:'loras',name:'scan.pt',path:'/models/loras/scan.pt',size:5,info:null}]:c==='native_model_scan_status'?new Promise(r=>{resolve=r;}):null);
 const view=render(<I18nProvider><DownloadPanel embedded installTarget="/env" modelsDir="/models"/></I18nProvider>);
 await act(async()=>{});tab('Installed');await act(async()=>{});expect(resolve).toBeTypeOf('function');view.unmount();
 await act(async()=>{resolve({id:'job',state:'running',completed:0,total:1,remaining:1,failed:0});await vi.advanceTimersByTimeAsync(5000);});
 expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_model_scan_status')).toHaveLength(1);
});
