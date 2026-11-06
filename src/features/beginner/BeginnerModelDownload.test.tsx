import {afterEach,beforeEach,expect,test,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {BeginnerModelDownload} from './BeginnerModelDownload';
import {I18nProvider} from '../../i18n/I18nProvider';
import {MODEL_GUIDES} from './modelGuides';
import {invoke} from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(async()=>null),isTauri:()=>true}));
import {browseDownloads,planDownloads,startDownloads} from '../../lib/downloads';
vi.mock('../../lib/downloads',async original=>({...await original<typeof import('../../lib/downloads')>(),browseDownloads:vi.fn(),planDownloads:vi.fn(),startDownloads:vi.fn(),readAuthStatus:vi.fn(async()=>[]),readDownloadStatus:vi.fn(),cancelDownloads:vi.fn()}));
const resource={id:'sft',label:'moss_image_sft_bf16.safetensors',kind:'file',locator:'diffusion_models/moss_image_sft_bf16.safetensors',downloadable:true};
beforeEach(()=>{
 vi.mocked(browseDownloads).mockResolvedValue({resources:[resource,{...resource,id:'base',label:'moss_image_base_bf16.safetensors',locator:'diffusion_models/moss_image_base_bf16.safetensors'}],nextCursor:null,warnings:[]});
 vi.mocked(planDownloads).mockResolvedValue({id:'native-plan',target:'/env/ComfyUI/models/diffusion_models',resources:[resource],totalKnownBytes:10,unknownSizeCount:0,warnings:[],canExecute:true});
 vi.mocked(startDownloads).mockResolvedValue({state:'succeeded',completed:1,total:1,bytesDownloaded:10,error:null});
});
afterEach(()=>{cleanup();vi.clearAllMocks();});
test('SFT browsing is read-only; review and confirm use the existing native plan and execution boundary',async()=>{
 const status=vi.fn();render(<I18nProvider><BeginnerModelDownload guide={MODEL_GUIDES[1]} root="/env" onStatus={status}/></I18nProvider>);
 await screen.findByRole('option',{name:resource.label});
 expect(browseDownloads).toHaveBeenCalledWith({providerId:'huggingface',scope:'repo',locator:'models:Comfy-Org/MOSS-Image',authenticated:false});
 expect(screen.queryByRole('option',{name:'moss_image_base_bf16.safetensors'})).toBeNull();
 expect(planDownloads).not.toHaveBeenCalled();expect(startDownloads).not.toHaveBeenCalled();
 fireEvent.change(screen.getByRole('combobox',{name:'Model file'}),{target:{value:'sft'}});
 fireEvent.click(screen.getByRole('button',{name:'Review download'}));
 const confirm=await screen.findByRole('button',{name:'Confirm download'});
 expect(planDownloads).toHaveBeenCalledWith({providerId:'huggingface',resources:[resource],parentPath:'/env/ComfyUI/models/diffusion_models',folderName:'',authenticated:false});
 expect(startDownloads).not.toHaveBeenCalled();fireEvent.click(confirm);fireEvent.click(confirm);
 await waitFor(()=>expect(startDownloads).toHaveBeenCalledTimes(1));expect(startDownloads).toHaveBeenCalledWith('native-plan');
 await waitFor(()=>expect(status).toHaveBeenCalledWith(expect.objectContaining({state:'succeeded'})));
});
test('a native plan rejection cannot start a download',async()=>{
 vi.mocked(planDownloads).mockResolvedValue({id:'blocked',target:'/m',resources:[resource],totalKnownBytes:0,unknownSizeCount:1,warnings:['Existing file will not be overwritten.'],canExecute:false});
 render(<I18nProvider><BeginnerModelDownload guide={MODEL_GUIDES[1]} modelsDir="/m" onStatus={()=>{}}/></I18nProvider>);
 await screen.findByRole('option',{name:resource.label});fireEvent.change(screen.getByRole('combobox',{name:'Model file'}),{target:{value:'sft'}});fireEvent.click(screen.getByRole('button',{name:'Review download'}));
 expect(await screen.findByText('Existing file will not be overwritten.')).toBeTruthy();
 expect(screen.getByRole('button',{name:'Confirm download'})).toHaveProperty('disabled',true);expect(startDownloads).not.toHaveBeenCalled();
});
test('browse errors produce a fixed central diagnostic without private error text',async()=>{
 vi.mocked(browseDownloads).mockRejectedValue(new Error('private-token-in-provider-error'));
 render(<I18nProvider><BeginnerModelDownload guide={MODEL_GUIDES[1]} modelsDir="/m" onStatus={()=>{}}/></I18nProvider>);
 expect(await screen.findByRole('alert')).toBeTruthy();
 expect(invoke).toHaveBeenCalledWith('native_logs_beginner_error',{operation:'download'});
 expect(screen.queryByText(/private-token/)).toBeNull();expect(startDownloads).not.toHaveBeenCalled();
});
