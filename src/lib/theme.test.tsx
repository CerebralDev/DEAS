import {afterEach,beforeEach,expect,test,vi} from 'vitest';
import {cleanup,render,screen,fireEvent,waitFor} from '@testing-library/react';
import {invoke} from '@tauri-apps/api/core';
import {availableSchemes,applySavedScheme,applyScheme,refreshThemePacks,useThemeCatalog,useScheme,type ThemePack} from './theme';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:()=>true}));
const pack:ThemePack={id:'sage',name:'Sage',author:'Owner',base:'default',colors:{panel:'#123456',ok:'#abc'},assets:{cursor:'data:image/png;base64,YQ=='},presentation:null,effects:{mouse:false,wine:false}};
const catalog=(packs:ThemePack[])=>({directory:'/env/themes',packs,warnings:[]});
function Probe({root='/env'}:{root?:string}){const state=useThemeCatalog(root);const [scheme,choose]=useScheme();return <><span>{state.active?.name??'Built-in'}</span><button onClick={()=>choose('sage')}>{scheme}</button></>;}
beforeEach(async()=>{localStorage.clear();vi.mocked(invoke).mockReset();await refreshThemePacks();applyScheme('default');});
afterEach(()=>cleanup());
test('legacy ID loads its external pack; colors, RGB companions and images clear on built-in switch',async()=>{
 localStorage.setItem('deas.pref.appearance.scheme',JSON.stringify('sage'));
 vi.mocked(invoke).mockResolvedValue(catalog([pack]));render(<Probe/>);
 await screen.findByText('Sage');expect(document.documentElement.style.getPropertyValue('--panel')).toBe('#123456');
 expect(document.documentElement.style.getPropertyValue('--ok-rgb')).toBe('170 187 204');expect(document.documentElement.dataset.themeCursor).toBe('true');
 applyScheme('default');expect(document.documentElement.style.getPropertyValue('--panel')).toBe('');expect(document.documentElement.dataset.themePack).toBeUndefined();expect(document.documentElement.dataset.themeCursor).toBeUndefined();
});
test('custom IDs survive reload and absence falls back without erasing the choice; restoring the pack reapplies it',async()=>{
 const custom={...pack,id:'my-pack',name:'My pack'};localStorage.setItem('deas.pref.appearance.scheme',JSON.stringify('my-pack'));
 vi.mocked(invoke).mockResolvedValue(catalog([custom]));await refreshThemePacks('/env');applySavedScheme();expect(document.documentElement.dataset.themePack).toBe('my-pack');
 vi.mocked(invoke).mockResolvedValue(catalog([]));await refreshThemePacks('/env');expect(document.documentElement.dataset.themePack).toBeUndefined();expect(JSON.parse(localStorage.getItem('deas.pref.appearance.scheme')!)).toBe('my-pack');
 vi.mocked(invoke).mockResolvedValue(catalog([custom]));await refreshThemePacks('/env');expect(document.documentElement.dataset.themePack).toBe('my-pack');
});
test('a late scan cannot replace the current environment',async()=>{
 let resolve:(v:unknown)=>void=()=>{};vi.mocked(invoke).mockImplementationOnce(()=>new Promise(r=>{resolve=r;}));
 const first=refreshThemePacks('/old');vi.mocked(invoke).mockResolvedValue(catalog([]));await refreshThemePacks('/new');resolve(catalog([pack]));await first;
 applyScheme('sage');expect(document.documentElement.dataset.themePack).toBeUndefined();
});
test('failed scans clear custom images and retain Default recovery without resurrecting packs',async()=>{
 localStorage.setItem('deas.pref.appearance.scheme',JSON.stringify('sage'));
 vi.mocked(invoke).mockResolvedValue(catalog([pack]));render(<Probe/>);await screen.findByText('Sage');fireEvent.click(screen.getByRole('button'));
 vi.mocked(invoke).mockRejectedValue(new Error('native unavailable'));await refreshThemePacks('/env');
 await waitFor(()=>expect(document.documentElement.dataset.themePack).toBeUndefined());expect(document.documentElement.dataset.theme).toBeUndefined();
 expect(availableSchemes().map(s=>s.id)).toEqual(['default']);
});

test('deleting selected Android White removes its menu entry and palette on refresh; restoration reapplies it',async()=>{
 const android={...pack,id:'android-white',name:'Android White',base:'android-white'};
 localStorage.setItem('deas.pref.appearance.scheme',JSON.stringify(android.id));
 vi.mocked(invoke).mockResolvedValue(catalog([android]));await refreshThemePacks('/env');
 expect(availableSchemes().some(s=>s.id===android.id)).toBe(true);
 expect(document.documentElement.dataset.theme).toBe(android.id);
 vi.mocked(invoke).mockResolvedValue(catalog([]));await refreshThemePacks('/env');
 expect(availableSchemes().map(s=>s.id)).toEqual(['default']);
 expect(document.documentElement.dataset.theme).toBeUndefined();
 expect(document.documentElement.style.getPropertyValue('--panel')).toBe('');
 applySavedScheme();expect(document.documentElement.dataset.theme).toBeUndefined();
 expect(JSON.parse(localStorage.getItem('deas.pref.appearance.scheme')!)).toBe(android.id);
 vi.mocked(invoke).mockResolvedValue(catalog([android]));await refreshThemePacks('/env');
 expect(document.documentElement.dataset.themePack).toBe(android.id);
});

test('onboarding keeps classic choices before an environment is selected',()=>{
 expect(availableSchemes()).toHaveLength(8);
 applyScheme('android-white');expect(document.documentElement.dataset.theme).toBe('android-white');
});

test('fresh installs keep Default and legacy VOID keeps its persisted ID and display name',()=>{
 applySavedScheme();expect(document.documentElement.dataset.theme).toBeUndefined();
 expect(availableSchemes().find(s=>s.id==='voud')?.name).toBe('VOID');
 localStorage.setItem('deas.pref.appearance.scheme',JSON.stringify('voud'));
 applySavedScheme();expect(document.documentElement.dataset.theme).toBe('voud');
});

test('pack styles reject CSS and URL injection and clear every applied property when environments change',async()=>{
 const unsafe={...pack,colors:{panel:'url(https://example.com)',ok:'#01020380','bad;key':'#fff'},assets:{background:'url(https://example.com)',cursor:'data:image/svg+xml;base64,YQ=='}};
 vi.mocked(invoke).mockResolvedValue(catalog([unsafe]));await refreshThemePacks('/env');applyScheme('sage');
 expect(document.documentElement.style.getPropertyValue('--panel')).toBe('');
 expect(document.documentElement.style.getPropertyValue('--ok-rgb')).toBe('1 2 3');
 expect(document.documentElement.style.getPropertyValue('--bad;key')).toBe('');
 expect(document.documentElement.dataset.themeCursor).toBeUndefined();
 expect(document.documentElement.style.getPropertyValue('--theme-asset-background')).toBe('');
 vi.mocked(invoke).mockResolvedValue(catalog([]));await refreshThemePacks('/other');
 expect(document.documentElement.style.getPropertyValue('--ok')).toBe('');
 expect(document.documentElement.style.getPropertyValue('--ok-rgb')).toBe('');
});

test('validated WebP assets render and unsupported image URLs remain inert',async()=>{
 const webp={...pack,assets:{background:'data:image/webp;base64,YQ==',cursor:'data:image/webp;base64,YQ==',bad:'https://example.com/picture.png'}};
 vi.mocked(invoke).mockResolvedValue(catalog([webp]));await refreshThemePacks('/env');applyScheme('sage');
 expect(document.documentElement.style.getPropertyValue('--theme-asset-background')).toContain('data:image/webp;base64,');
 expect(document.documentElement.dataset.themeCursor).toBe('true');
 expect(document.documentElement.style.getPropertyValue('--theme-asset-bad')).toBe('');
 applyScheme('default');expect(document.documentElement.style.getPropertyValue('--theme-asset-background')).toBe('');
});
