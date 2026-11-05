import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, test, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import { QuickSettings } from './QuickSettings';
import { applySavedScheme, refreshThemePacks } from '../../lib/theme';
import { revealTarget, searchIndex } from '../../lib/search';

vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:vi.fn(()=>false)}));
beforeEach(async()=>{vi.mocked(invoke).mockReset();vi.mocked(isTauri).mockReturnValue(false);await refreshThemePacks();});

test('Quick Settings exposes separate layout and authoring reset requests and closes with Escape',()=>{
 localStorage.setItem('deas.locale.v1','en');
 const layout=vi.fn(),defaults=vi.fn();window.addEventListener('deas:reset-studio-layout',layout);window.addEventListener('deas:request-studio-defaults',defaults);
 const target=document.createElement('div');target.setAttribute('data-workbench','');document.body.append(target);
 const view=render(<I18nProvider><QuickSettings/></I18nProvider>);
 const trigger=screen.getByRole('button',{name:'Quick Settings'});
 fireEvent.click(trigger);fireEvent.click(screen.getByRole('button',{name:'Reset layout'}));
 expect(layout).toHaveBeenCalledOnce();expect(defaults).not.toHaveBeenCalled();expect(trigger.getAttribute('aria-expanded')).toBe('false');
 fireEvent.click(trigger);fireEvent.click(screen.getByRole('button',{name:'Reset to defaults…'}));expect(defaults).toHaveBeenCalledOnce();
 fireEvent.click(trigger);fireEvent.keyDown(window,{key:'Escape'});expect(trigger.getAttribute('aria-expanded')).toBe('false');expect(document.activeElement).toBe(trigger);
 view.unmount();target.remove();window.removeEventListener('deas:reset-studio-layout',layout);window.removeEventListener('deas:request-studio-defaults',defaults);
});

test('reset actions explain why they are unavailable before Studio mounts',()=>{
 localStorage.setItem('deas.locale.v1','en');
 const request=vi.fn();window.addEventListener('deas:reset-studio-layout',request);window.addEventListener('deas:request-studio-defaults',request);
 const view=render(<I18nProvider><QuickSettings/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Quick Settings'}));
 for(const name of ['Reset layout','Reset to defaults…']){
  const action=screen.getByRole('button',{name});
  expect(action.hasAttribute('disabled')).toBe(true);
  expect(document.getElementById(action.getAttribute('aria-describedby')!)?.textContent).toBe('Open Studio first to enable reset actions.');
  fireEvent.click(action);
 }
 expect(request).not.toHaveBeenCalled();
 view.unmount();window.removeEventListener('deas:reset-studio-layout',request);window.removeEventListener('deas:request-studio-defaults',request);
});

test('a colour scheme applies at once, is remembered and survives a reload; Default removes it',()=>{
 localStorage.setItem('deas.locale.v1','en');localStorage.setItem('deas.pref.appearance.scheme',JSON.stringify('voud'));applySavedScheme();
 const view=render(<I18nProvider><QuickSettings/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Quick Settings'}));
 const schemes=screen.getByRole('group',{name:'Colour scheme'});
 expect([...schemes.querySelectorAll('button')].map(b=>b.textContent)).toEqual(['Default','Liiti Pink','VOID','Alluring Abyss','Slimy Teal','Clawy Grey','Android White',"Dragon's Hoard"]);
 expect(screen.getByRole('button',{name:'VOID'}).getAttribute('aria-pressed')).toBe('true');
 fireEvent.click(screen.getByRole('button',{name:'VOID'}));
 expect(document.documentElement.dataset.theme).toBe('voud');
 expect(JSON.parse(localStorage.getItem('deas.pref.appearance.scheme')!)).toBe('voud');
 expect(screen.getByRole('button',{name:'VOID'}).getAttribute('aria-pressed')).toBe('true');
 delete document.documentElement.dataset.theme;applySavedScheme();expect(document.documentElement.dataset.theme).toBe('voud');
 fireEvent.click(screen.getByRole('button',{name:'Alluring Abyss'}));
 expect(document.documentElement.dataset.theme).toBe('alluring-abyss');
 expect(JSON.parse(localStorage.getItem('deas.pref.appearance.scheme')!)).toBe('alluring-abyss');
 expect(screen.getByRole('button',{name:'Alluring Abyss'}).getAttribute('aria-pressed')).toBe('true');
 delete document.documentElement.dataset.theme;applySavedScheme();expect(document.documentElement.dataset.theme).toBe('alluring-abyss');
 fireEvent.click(screen.getByRole('button',{name:'Default'}));
 expect(document.documentElement.dataset.theme).toBeUndefined();
 view.unmount();
});

test('theme controls are searchable before opening Quick Settings and unavailable without a native environment',async()=>{
 localStorage.setItem('deas.locale.v1','en');
 const view=render(<I18nProvider><QuickSettings/></I18nProvider>);
 for(const key of ['theme.ui.refresh','theme.ui.openThemesFolder']){
  const entry=searchIndex.find(e=>e.label===key)!;expect(entry).toBeTruthy();
  act(()=>{revealTarget(entry.target);});
  expect(revealTarget(entry.target).state).toBe('disabled');
 }
 expect(screen.getByRole('button',{name:'Quick Settings'}).getAttribute('aria-expanded')).toBe('true');
 expect(invoke).not.toHaveBeenCalled();view.unmount();
});

test('refresh reloads the environment catalog and opening its folder reports failure without hiding it behind a scan',async()=>{
 localStorage.setItem('deas.locale.v1','en');vi.mocked(isTauri).mockReturnValue(true);
 vi.mocked(invoke).mockImplementation(async command=>{
  if(command==='native_theme_packs')return {directory:'/env/themes',packs:[],warnings:[]};
  if(command==='native_theme_folder')throw Error('folder unavailable');
  if(command==='native_spellcheck_status')return {available:true,installed:true};
  throw Error('Unexpected fixture IPC '+command);
 });
 const view=render(<I18nProvider><QuickSettings root="/env"/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Quick Settings'}));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Refresh themes'}).matches(':disabled')).toBe(false));
 expect(invoke).toHaveBeenCalledWith('native_theme_packs',{root:'/env'});
 fireEvent.click(screen.getByRole('button',{name:'Refresh themes'}));
 await waitFor(()=>expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_theme_packs')).toHaveLength(2));
 fireEvent.click(screen.getByRole('button',{name:'Open themes folder'}));
 await screen.findByText('Could not open the themes folder. See Logs.');
 expect(invoke).toHaveBeenCalledWith('native_theme_folder',{root:'/env'});
 expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_theme_packs')).toHaveLength(2);
 view.unmount();await refreshThemePacks();
});
