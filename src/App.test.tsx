import { defaultLlmConfig, llmSetupEvent, type LlmModels } from './lib/llm';
import { readAuthStatus } from './lib/downloads';
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  cleanup,
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import App from "./App";
import { I18nProvider, LOCALE_STORAGE_KEY } from "./i18n/I18nProvider";
import { inspectBackend } from "./lib/comfy";
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { NativeInstallRequest } from './lib/setup';
vi.mock("./lib/comfy", () => ({ inspectBackend: vi.fn() }));
// Backend embeds auth now; lifecycle fixtures provide its real empty metadata schema.
vi.mock('./lib/downloads',async original=>({...await original<typeof import('./lib/downloads')>(),readAuthStatus:vi.fn(async()=>[])}));
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:vi.fn(()=>false)}));
vi.mock('@tauri-apps/api/webview',()=>({getCurrentWebview:()=>({onDragDropEvent:async()=>()=>{}})}));

function renderStudio() {
  localStorage.setItem(LOCALE_STORAGE_KEY, "en");
  const view = render(<I18nProvider><App /></I18nProvider>);
  fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
  fireEvent.click(screen.getByRole("tab", { name: "Run Plan" }));
  fireEvent.click(screen.getByText('Engine diagnostics & legacy workspace'));
  return view;
}

beforeEach(()=>vi.mocked(readAuthStatus).mockResolvedValue([]));

afterEach(() => {
  cleanup();
  window.localStorage.clear();
  vi.resetAllMocks();
});

test("language selection updates app, setup, studio metadata and aria without losing local state", () => {
  renderStudio();
  fireEvent.click(screen.getByRole("button", { name: /^Director$/i }));
  fireEvent.change(screen.getByLabelText("Shot direction"), { target: { value: "Preserve my draft" } });
  // Language lives in Settings; a pick there is marked as a custom choice.
  fireEvent.click(screen.getByRole("tab", { name: "Backend" }));
  fireEvent.change(screen.getByLabelText("Language"), { target: { value: "de" } });
  expect(document.documentElement.lang).toBe("de");
  expect(screen.getByText("Eigene Wahl")).toBeTruthy();
  fireEvent.click(screen.getByRole("tab", { name: "Studio" }));
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));expect(screen.getByRole('region',{name:'Provider-Autorisierung'})).toBeTruthy();fireEvent.click(document.querySelector('[data-studio-tab="map"]')!);
  expect(screen.getByLabelText("Aufnahmeregie")).toHaveProperty("value", "Preserve my draft");
  expect(screen.getByRole("button", { name: "Director schließen" })).toBeTruthy();
  expect(document.querySelector(".neural-legend")).toBeNull();
  expect([...document.querySelectorAll('[data-node="director"] [data-included]')].map(part=>part.textContent)).toEqual(["Prompt Forge", "RefMods", "Kontinuität", "Ausgabe"]);
  expect(screen.getByLabelText("Bildausschnitt").textContent).toContain("Totale");
  fireEvent.click(screen.getByRole("tab", { name: "Backend" }));
  fireEvent.change(screen.getByLabelText("Sprache"), { target: { value: "en" } });
  fireEvent.click(screen.getByRole("tab", { name: "Run Plan" }));
  expect(screen.getByLabelText("Shot direction")).toHaveProperty("value", "Preserve my draft");
  expect(screen.getByLabelText("Framing")).toHaveProperty("value", "wide");
});

test('footer version opens Backend and requests consent without starting an update',()=>{
 localStorage.setItem(LOCALE_STORAGE_KEY,'en');
 render(<I18nProvider><App/></I18nProvider>);
 fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
 fireEvent.click(screen.getByRole('button',{name:/DEAS v/}));
 expect(screen.getByRole('tab',{name:'Backend',selected:true})).toBeTruthy();
 expect(screen.getByRole('alertdialog').textContent).toContain('Update DEAS?');
 expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_app_update'||c==='native_install_update')).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'Cancel'}));
 expect(screen.queryByRole('alertdialog')).toBeNull();
 expect((screen.getByRole('button',{name:'Upgrade'}) as HTMLButtonElement).disabled).toBe(true);
});

test('Generate preserves the same workbench across map and top-level tab navigation',()=>{
 localStorage.setItem(LOCALE_STORAGE_KEY,'en');
 const view=render(<I18nProvider><App/></I18nProvider>);
 fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
 const workbench=view.container.querySelector('[data-workbench]');
 expect(workbench).toBeTruthy();
 fireEvent.click(screen.getByRole('tab',{name:'Run Plan'}));
 expect(view.container.querySelector('[data-workbench]')).toBe(workbench);
 fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
 expect(view.container.querySelector('[data-workbench]')).toBe(workbench);
 fireEvent.click(screen.getByRole('tab',{name:'Library'}));
 fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
 expect(view.container.querySelector('[data-workbench]')).toBe(workbench);
});

test('TVE mounts only on first visit, preserves source/output and image Post drafts across navigation without navigation IPC',async()=>{
 localStorage.setItem(LOCALE_STORAGE_KEY,'en');
 const view=render(<I18nProvider><App/></I18nProvider>);
 fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
 expect(document.querySelector('[data-tve]')).toBeNull();
 const workbench=view.container.querySelector('[data-workbench]');
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 const upscale=screen.getByLabelText('Upscale factor');
 fireEvent.change(upscale,{target:{value:'3'}});
 expect(document.querySelector('[data-video-post]')).toBeNull();
 const forgeOpen=vi.fn(); window.addEventListener('deas:forge-open',forgeOpen);
 try{
  vi.mocked(invoke).mockClear();
  // Mounting TVE does no TVE native work even in the desktop app; only the explicit inspect/enhance actions do.
  vi.mocked(isTauri).mockReturnValue(true);
  const source={name:'kept.mp4',width:1920,height:1080,fpsNumerator:24000,fpsDenominator:1001,durationSeconds:12.5,pixelFormat:'yuv420p',fieldOrder:'progressive',colorTransfer:'bt709',audioStreams:1,subtitleStreams:0,blockers:[],timingVerified:false,processingReady:false};
  vi.mocked(invoke).mockImplementation(async command=>{
   if(command==='native_video_post_inspect')return source;
   if(command==='native_recipe_prerequisites')return {baseUrl:'http://127.0.0.1:7801/',recipeId:'checkpoint-text-to-image-diagnostic',version:1,allowlist:{nodes:[],checkpoints:[],loras:[],images:[],samplers:[],schedulers:[]},missingNodes:[],generationReady:false};
   return null; // App-level IPC on the desktop flag (update check, repo stars) is not TVE work.
  });
  fireEvent.click(screen.getByRole('button',{name:'Open TVE video workspace'}));
  expect(vi.mocked(invoke).mock.calls.map(c=>String(c[0])).filter(c=>c.startsWith('native_video_post')||c.startsWith('native_workbench')||c==='native_recipe_prerequisites')).toEqual([]);
  expect(inspectBackend).not.toHaveBeenCalled();
  expect(forgeOpen).not.toHaveBeenCalled();
  expect(screen.getByRole('tab',{name:'TVE',selected:true})).toHaveProperty('title','TrueVideoEnhancer — video post-processing');
  expect(workbench?.closest('[hidden]')).toBeTruthy();
  expect(document.querySelectorAll('[data-video-post]')).toHaveLength(1);
  await act(async()=>fireEvent.click(document.querySelector('[data-video-post-inspect]')!));
  fireEvent.click(document.querySelector('[data-tve-post="denoise"]')!);
  const tve=document.querySelector('[data-tve]'),sourceCard=document.querySelector('[data-video-post-source]'),draft=document.querySelector('[data-tve-backend]');
  expect(sourceCard).toBeTruthy();expect(draft).toBeTruthy();
  vi.mocked(isTauri).mockReturnValue(false); vi.mocked(invoke).mockClear();
  for(const name of ['Library','Backend','Studio','Run Plan','TVE'])fireEvent.click(screen.getByRole('tab',{name}));
  expect(document.querySelector('[data-tve]')).toBe(tve);
  expect(document.querySelector('[data-video-post-source]')).toBe(sourceCard);
  expect(document.querySelector('[data-tve-backend]')).toBe(draft);expect((document.querySelector('[data-tve-post="denoise"]') as HTMLInputElement).checked).toBe(true);
  expect(document.querySelector('[data-workbench]')).toBe(workbench);
  expect(invoke).not.toHaveBeenCalled();expect(forgeOpen).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
  expect(screen.getByLabelText('Upscale factor')).toBe(upscale);expect(upscale).toHaveProperty('value','3');
 }finally{window.removeEventListener('deas:forge-open',forgeOpen);}
});

test('persisted TVE selection is accepted without source inspection on reopening',()=>{
 localStorage.setItem(LOCALE_STORAGE_KEY,'en');localStorage.setItem('deas.pref.studio.tab','"tve"');
 render(<I18nProvider><App/></I18nProvider>);
 expect(localStorage.getItem('deas.pref.studio.tab')).toBe('"tve"');
 expect(document.querySelector('[data-tve]')).toBeNull();
 expect(invoke).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('tab',{name:'TVE'}));
 expect(screen.getByRole('tab',{name:'TVE',selected:true})).toBeTruthy();
 expect(document.querySelector('[data-tve]')).toBeTruthy();
 expect(document.querySelector('[data-video-post-source]')).toBeNull();
 expect(invoke).not.toHaveBeenCalled();
});

test.each(['Library','Comfy','Backend','Logs','TVE',null])('deferred missing Forge config respects the actual top-level view (%s)',async destination=>{
 localStorage.setItem(LOCALE_STORAGE_KEY,'en');
 localStorage.setItem('deas.pref.install.root','"/env"');
 localStorage.setItem('deas.pref.backend.autostart','false');
 vi.mocked(isTauri).mockReturnValue(true);
 let finish!:(value:LlmModels)=>void;
 const read=new Promise<LlmModels>(resolve=>{finish=resolve;});
 vi.mocked(invoke).mockImplementation(async(command,args)=>{
  if(command==='native_install_defaults')return {storagePath:'/env',backend:'cpu',warnings:[]};
  if(command==='native_files_present')return [true];
  if(command==='native_install_plan')return {id:'restore',reopenId:'restore',request:(args as {request:NativeInstallRequest}).request,steps:[],warnings:[],unsupportedOptions:[],canExecute:true};
  if(command==='native_install_open')return {state:'succeeded',stepId:null,logs:[],error:null};
  if(command==='native_settings_read')return {};
  if(command==='native_backend_status')return {state:'stopped',logs:[],error:null};
  if(command==='native_models_scan')return [];
  if(command==='native_media_list')return {path:'/env',parent:null,entries:[],truncated:false};
  if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};
  if(command==='native_llm_read')return defaultLlmConfig;
  if(command==='native_llm_models')return read;
  throw Error('Unexpected fixture IPC '+command);
 });
 const redirected=vi.fn();window.addEventListener(llmSetupEvent,redirected);
 try{
  const view=render(<I18nProvider><App/></I18nProvider>);
  await screen.findByRole('tab',{name:'Studio',selected:true});
  fireEvent.click(screen.getByRole('tab',{name:'Prompt Forge'}));
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_llm_models',{root:'/env'}));
  // The result panel is an empty state until something is written; the brief is the draft that must survive.
  const result=screen.getByRole('textbox',{name:'Idea or prompt to enhance'});
  fireEvent.change(result,{target:{value:'retained Forge result'}});
  const workbench=view.container.querySelector('[data-workbench]');
  if(destination)fireEvent.click(screen.getByRole('tab',{name:destination}));
  await act(async()=>finish({models:[],warnings:[]}));
  expect(screen.getByRole('tab',{name:destination??'Backend',selected:true})).toBeTruthy();
  if(destination){
   expect(redirected).not.toHaveBeenCalled();
   expect(view.container.querySelector('[data-backend-panel="llm"]')?.getAttribute('aria-selected')).toBe('false');
   expect(screen.queryByText(/Your draft has been kept/)).toBeNull();
  }else{
   expect(redirected).toHaveBeenCalledOnce();
   expect(screen.getByRole('tab',{name:'LLM backend',selected:true})).toBeTruthy();
   expect(screen.getByText(/Your draft has been kept/)).toBeTruthy();
  }
  expect(result.isConnected).toBe(true);expect(result).toHaveProperty('value','retained Forge result');
  expect(result.closest('[hidden]')).not.toBeNull();
  expect(view.container.querySelector('[data-workbench]')).toBe(workbench);
 }finally{window.removeEventListener(llmSetupEvent,redirected);}
});

test("first entry shows only AiO and keeps the studio behind an honest preview control", () => {
  localStorage.setItem(LOCALE_STORAGE_KEY, "en");
  const view = render(<I18nProvider><App /></I18nProvider>);
  expect([...view.container.querySelectorAll("[data-node]")].map(n=>n.getAttribute("data-node"))).toEqual(["studio","optional-downloads","comfy","settings","logs"]);
  expect(screen.queryByRole("group", { name: "Connected studio features" })).toBeNull();
  expect(inspectBackend).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('tab',{name:'Studio'}));
  expect(screen.getByRole("tab", { name: "Studio", selected: true })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Run" })).toBeTruthy();
  expect(screen.getByRole("complementary", { name: "Media" })).toBeTruthy();
  expect(screen.queryByRole("group", { name: "Connected studio features" })).toBeNull();
  fireEvent.click(screen.getByRole("tab", { name: "Run Plan" }));
  expect(screen.getByRole('region', {name:'Run Plan'})).toBeTruthy();
  expect(inspectBackend).not.toHaveBeenCalled();
});

test.each([false,true])("successful native install reveals studio without an extra enable action (Director: %s)", async (director) => {
  localStorage.setItem('deas.pref.backend.autostart','false');
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke).mockImplementation(async(command,args)=>{if(command==='native_theme_packs')return {directory:'/fixture/env/themes',packs:[],warnings:[]};if(command==='native_llm_read')return defaultLlmConfig;if(command==='native_llm_models')return {models:[],warnings:[]};
    if(command==='native_install_defaults')return {storagePath:'/app/new',backend:'cpu',warnings:[]};
    if(command==='native_workbench_current')return {jobs:[],preparing:0,capacity:20,stopAvailable:true,admissionEpoch:0,stopping:false};
    if(command==='native_models_scan')return [];
    if(command==='native_media_list')return {path:'/home/fixture',parent:null,entries:[],truncated:false};
    if(command==='native_install_plan')return {id:'installed-plan',request:(args as {request:NativeInstallRequest}).request,steps:[],warnings:[],unsupportedOptions:[],canExecute:true};
    return {state:'succeeded',stepId:null,logs:[],error:null};
  });
  localStorage.setItem(LOCALE_STORAGE_KEY, "en");
  render(<I18nProvider><App /></I18nProvider>);
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));
  if(!director)fireEvent.click(screen.getByLabelText('Director'));
  await waitFor(()=>expect(screen.getByLabelText('Storage directory')).toHaveProperty('value','/app/new'));
  const start=await screen.findByRole('button',{name:'Start install'});
  expect(screen.queryByRole('tablist',{name:'Studio areas'})).toBeNull();
  fireEvent.click(start);
  expect(await screen.findByRole('complementary',{name:'Media'})).toBeTruthy();
  fireEvent.click(screen.getByRole('tab',{name:'Run Plan'}));
  expect(screen.getByRole('region',{name:'Run Plan'})).toBeTruthy();
  fireEvent.click(screen.getByText('Engine diagnostics & legacy workspace'));
  fireEvent.click(screen.getByRole('button',{name:'Director'}));
  expect(screen.getByLabelText('Shot direction')).toBeTruthy();
  expect(inspectBackend).not.toHaveBeenCalled();
  expect(screen.queryByRole('button',{name:/generate/i})).toBeNull();
  expect(vi.mocked(invoke).mock.calls.map(([command])=>command).filter(c=>!['native_theme_packs','native_spellcheck_status','native_auth_status','native_repo_stars','native_update_check','native_models_scan','native_media_list','native_audio_enhance_capabilities'].includes(c))).toEqual(['native_install_defaults','native_install_plan','native_install_start','native_wildcard_library','native_workbench_current']); // read-only session recovery is explicit; media/model listings of the installed root are timing-dependent and asserted in their own suites
});

test("existing setup handoff reveals Engine and prefills its endpoint without inspection", () => {
  localStorage.setItem(LOCALE_STORAGE_KEY, "en");
  render(<I18nProvider><App /></I18nProvider>);
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));
  fireEvent.change(screen.getByLabelText("Existing backend address"), { target: { value: "http://127.0.0.1:8188" } });
  fireEvent.click(screen.getByRole("button", { name: "Connect existing backend" }));
  expect(screen.getByRole("button", { name: /^Engine$/ }).getAttribute("aria-expanded")).toBe("true");
  expect(screen.getByLabelText("Backend URL")).toHaveProperty("value", "http://127.0.0.1:8188");
  expect(document.activeElement).toBe(screen.getByLabelText("Backend URL"));
  expect(inspectBackend).not.toHaveBeenCalled();
});

test("a setup handoff discards an in-flight snapshot from the previous address", async () => {
  let complete!: (result: Awaited<ReturnType<typeof inspectBackend>>) => void;
  vi.mocked(inspectBackend).mockReturnValue(new Promise((resolve) => { complete = resolve; }));
  renderStudio();
  fireEvent.click(screen.getByRole("button", { name: /^Engine$/ }));
  fireEvent.click(screen.getByRole("button", { name: /Inspect connection/ }));
  const setup = within(screen.getByRole("region", { name: "AiO setup planner" }));
  fireEvent.click(screen.getByRole('tab',{name:'Backend'}));
  fireEvent.change(setup.getByLabelText("Existing backend address"), { target: { value: "http://127.0.0.1:8188" } });
  fireEvent.click(setup.getByRole("button", { name: "Connect existing backend" }));
  await act(async () => complete({ baseUrl: "http://127.0.0.1:7801", nodeCount: 1, devices: ["Stale GPU"], missingRequiredNodes: [], capabilities: [] }));
  expect(screen.getByLabelText("Backend URL")).toHaveProperty("value", "http://127.0.0.1:8188");
  expect(screen.queryByText("Stale GPU")).toBeNull();
  expect(inspectBackend).toHaveBeenCalledTimes(1);
});

test("Director owns passive included parts instead of duplicate interactive studio nodes", () => {
  const view = renderStudio();
  const studio = view.container.querySelector(".neural-field")!;
  expect([...studio.querySelectorAll("[data-node]")].map(node=>node.getAttribute("data-node"))).toEqual(["engine", "director", "plugins"]);
  const director=studio.querySelector('[data-node="director"]')!;
  expect([...director.querySelectorAll("[data-included]")].map(part=>part.getAttribute("data-included"))).toEqual(["forge", "refmods", "continuity", "output"]);
  expect(studio.querySelectorAll("button")).toHaveLength(3);
  expect(studio.querySelector("[data-choice], .neural-legend, [data-kind=suggested]")).toBeNull();
  for(const part of director.querySelectorAll("[data-included]")) fireEvent.click(part);
  expect(view.container.querySelector("[id^='panel-']")).toBeNull();
  const trigger=screen.getByRole("button",{name:"Director"});
  fireEvent.click(trigger);
  fireEvent.change(screen.getByLabelText("Shot direction"),{target:{value:"Keep included direction"}});
  fireEvent.keyDown(view.container.querySelector(".panel-director")!, {key:"Escape"});
  expect(document.activeElement).toBe(trigger);
  fireEvent.click(trigger);
  expect(screen.getByLabelText("Shot direction")).toHaveProperty("value", "Keep included direction");
  expect(inspectBackend).not.toHaveBeenCalled();
});

test("feature nodes expand inline without making automatic backend requests", () => {
  renderStudio();
  const engine = screen.getByRole("button", {
    name: /^Engine$/i,
  });
  expect(engine.getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(engine);
  expect(engine.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getByLabelText("Backend URL")).toHaveProperty(
    "value",
    "http://127.0.0.1:7801",
  );
  expect(inspectBackend).not.toHaveBeenCalled();
});

test("explicit inspection displays real snapshot details and capability gaps", async () => {
  vi.mocked(inspectBackend).mockResolvedValue({
    baseUrl: "http://127.0.0.1:7801",
    nodeCount: 127,
    devices: ["Test GPU"],
    missingRequiredNodes: ["MiniMaxH3Director"],
    capabilities: ["supports_preview_metadata"],
  });
  renderStudio();
  fireEvent.click(
    screen.getByRole("button", { name: /^Engine$/i }),
  );
  fireEvent.click(screen.getByRole("button", { name: /Inspect connection/ }));
  await waitFor(() =>
    expect(inspectBackend).toHaveBeenCalledWith("http://127.0.0.1:7801"),
  );
  expect(await screen.findByText("Test GPU")).toBeTruthy();
  expect(screen.getByText("MiniMaxH3Director")).toBeTruthy();
  expect(screen.getByText("127")).toBeTruthy();
  expect(screen.getByText("supports_preview_metadata")).toBeTruthy();
});

test("Director saves only a local draft and restores it on reload", () => {
  const view = renderStudio();
  fireEvent.click(
    screen.getByRole("button", { name: /^Director$/i }),
  );
  fireEvent.change(screen.getByLabelText("Shot direction"), {
    target: { value: "Slow tracking shot through a copper-lit forest." },
  });
  fireEvent.click(within(document.querySelector(".panel-director") as HTMLElement).getByRole("button", { name: "Save local draft" }));
  expect(screen.getByText(/Draft saved on this device/)).toBeTruthy();
  expect(inspectBackend).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: /generate/i })).toBeNull();
  view.unmount();
  renderStudio();
  fireEvent.click(
    screen.getByRole("button", { name: /^Director$/i }),
  );
  expect(screen.getByLabelText("Shot direction")).toHaveProperty(
    "value",
    "Slow tracking shot through a copper-lit forest.",
  );
});

test("Plugins describes trusted bundled metadata without pretending to install extensions", () => {
  renderStudio();
  fireEvent.click(
    screen.getByRole("button", { name: /^Plugins$/i }),
  );
  expect(screen.getByText("Trusted, bundled metadata")).toBeTruthy();
  expect(
    screen.getByText("MiniMax H3 direction and guide capabilities."),
  ).toBeTruthy();
  expect(document.querySelector(".panel-plugins")?.querySelector(".action-button")).toBeNull();
});

test("closing an inline panel returns keyboard focus to its node", () => {
  renderStudio();
  const engine = screen.getByRole("button", {
    name: /^Engine$/i,
  });
  fireEvent.click(engine);
  const close = screen.getByRole("button", { name: "Close Engine" });
  close.focus();
  fireEvent.click(close);
  expect(document.activeElement).toBe(engine);
  expect(engine.getAttribute("aria-expanded")).toBe("false");
});

test.each([
 ['BACKEND_UNAVAILABLE', 'Backend not reachable'],
 ['BACKEND_TIMEOUT', 'did not respond in time'],
 ['BACKEND_REQUEST_FAILED', 'connection failed or was interrupted'],
 ['BACKEND_HTTP_ERROR', 'HTTP 503'],
])('inspection presents a human-readable %s without the raw transport URL', async (code, expected) => {
 vi.mocked(inspectBackend).mockRejectedValue(new Error(code === 'BACKEND_HTTP_ERROR' ? `${code}: object_info returned HTTP 503 Service Unavailable` : `${code}: object_info request failed http://127.0.0.1:45519/object_info`));
 renderStudio();
 fireEvent.click(screen.getByRole('button', { name: /^Engine$/i }));
 fireEvent.click(screen.getByRole('button', { name: /Inspect connection/ }));
 const alert = await screen.findByRole('alert');
 expect(alert.textContent).toContain(expected);
 expect(alert.textContent).not.toContain('object_info');
 expect(alert.textContent).not.toContain('45519');
 expect(screen.queryByText(/Inspection complete/)).toBeNull();
});

test("failed inspection shows actionable error instead of a connected state", async () => {
  vi.mocked(inspectBackend).mockRejectedValue(
    new Error("Backend unavailable. Check your URL and CORS settings."),
  );
  renderStudio();
  fireEvent.click(
    screen.getByRole("button", { name: /^Engine$/i }),
  );
  fireEvent.click(screen.getByRole("button", { name: /Inspect connection/ }));
  expect((await screen.findByRole("alert")).textContent).toContain(
    "Check your URL and CORS settings",
  );
  expect(screen.queryByText(/Inspection complete/)).toBeNull();
  expect(
    screen
      .getByRole("button", { name: /Inspect connection/ })
      .hasAttribute("disabled"),
  ).toBe(false);
});

test("editing the URL clears snapshot data from the previous backend", async () => {
  vi.mocked(inspectBackend).mockResolvedValue({
    baseUrl: "http://127.0.0.1:7801",
    nodeCount: 12,
    devices: ["Previous GPU"],
    missingRequiredNodes: [],
    capabilities: [],
  });
  renderStudio();
  fireEvent.click(
    screen.getByRole("button", { name: /^Engine$/i }),
  );
  fireEvent.click(screen.getByRole("button", { name: /Inspect connection/ }));
  expect(await screen.findByText("Previous GPU")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Backend URL"), {
    target: { value: "http://127.0.0.1:8188" },
  });
  expect(screen.queryByText("Previous GPU")).toBeNull();
  expect(inspectBackend).toHaveBeenCalledTimes(1);
});

test("invalid local draft data does not break the workspace", () => {
  window.localStorage.setItem("deas.director-draft.v1", "{bad-json");
  renderStudio();
  fireEvent.click(
    screen.getByRole("button", { name: /^Director$/i }),
  );
  expect(screen.getByLabelText("Shot direction")).toHaveProperty("value", "");
});

test("red banners get a close button that dismisses them", async () => {
  const { container } = renderStudio();
  const banner = document.createElement("p");
  banner.className = "error-message";
  banner.setAttribute("role", "alert");
  banner.textContent = "boom";
  container.querySelector(".studio-shell")!.appendChild(banner);
  const close = await within(banner).findByLabelText("Dismiss");
  fireEvent.click(close);
  expect(banner.classList.contains("is-dismissed")).toBe(true);
});

test("setup-style aio-error alerts are closeable too", async () => {
  const { container } = renderStudio();
  const banner = document.createElement("p");
  banner.className = "aio-error";
  banner.setAttribute("role", "alert");
  banner.textContent = "setup boom";
  container.querySelector(".studio-shell")!.appendChild(banner);
  const close = await within(banner).findByLabelText("Dismiss");
  fireEvent.click(close);
  expect(banner.classList.contains("is-dismissed")).toBe(true);
});
