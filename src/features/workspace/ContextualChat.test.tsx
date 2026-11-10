import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import { captureWorkspaceContext, cleanContextText, selectContext, suggestionFromText } from './context';
import { Suggestion } from './Suggestion';
import { isString, usePref } from '../../lib/preferences';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:()=>true,convertFileSrc:(p:string)=>p}));
beforeEach(()=>{localStorage.clear();localStorage.setItem('deas.locale.v1','en');vi.mocked(invoke).mockImplementation(async(c,a)=>c==='native_workspace_validate_suggestion'?(a as {proposed:unknown}).proposed:undefined);});
afterEach(()=>{cleanup();vi.resetAllMocks();});

function Controls(){const [v,setV]=usePref('studio.run.positive','original',isString);const [pipeline]=usePref('studio.run.pipeline','t2i',isString);return <><input aria-label="prompt" value={v} onChange={e=>setV(e.target.value)}/><span>{pipeline}</span></>;}

it('redacts sensitive paths and only shares relevant selected library metadata',()=>{
 selectContext('Library',{resource:'Motion',triggers:'dance',description:'C:\\private\\secrets sk-secret token=abc'});
 const snap=captureWorkspaceContext('Library');expect(snap.values.resource).toBe('Motion');expect(JSON.stringify(snap)).not.toMatch(/private|sk-secret|token=abc/);expect(snap.values).not.toHaveProperty('positive');
 expect(cleanContextText('/home/tim/secret')).toBe('[redacted]');expect(suggestionFromText('```deas-suggestion\n{"render":true}\n```')).toBeNull();expect(suggestionFromText('```deas-suggestion\n{"steps":8}\n```')).toEqual({steps:8});
});
it('suggestions require review, reject stale apply, preserve undo and never render',async()=>{
 render(<I18nProvider><Controls/><Suggestion proposed={{positive:'proposed'}}/></I18nProvider>);
 expect(invoke).not.toHaveBeenCalled();fireEvent.click(screen.getByText('Review changes'));fireEvent.change(screen.getByLabelText('prompt'),{target:{value:'newer'}});fireEvent.click(screen.getByText('Apply'));
 await screen.findByRole('alert');expect(screen.getByLabelText('prompt')).toHaveProperty('value','newer');
 // Re-review is explicit even after staleness.
 cleanup();render(<I18nProvider><Controls/><Suggestion proposed={{positive:'proposed'}}/></I18nProvider>);
 fireEvent.click(screen.getByText('Review changes'));fireEvent.click(screen.getByText('Apply'));await waitFor(()=>expect(screen.getByLabelText('prompt')).toHaveProperty('value','proposed'));
 fireEvent.click(screen.getByText('Undo'));expect(screen.getByLabelText('prompt')).toHaveProperty('value','newer');expect(vi.mocked(invoke).mock.calls.some(([c])=>/render|generate|submit/.test(c))).toBe(false);
});
