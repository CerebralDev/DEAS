import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { I18nProvider } from '../../i18n/I18nProvider';
import { WildcardPanel } from './WildcardPanel';
import { defaultWildcard, type WildcardLibrary } from '../../lib/wildcard';
const library:WildcardLibrary={Art:[{subject:'Lighting',booru_wildcards:['{red|blue} light'],nl_wildcards:['{Red|Blue} light.'],booru_presets:['cinematic'],nl_presets:['Cinematic.']}],'Negative Prompts':[{subject:'Blur',booru_presets:['blur']}]};
beforeEach(()=>{localStorage.clear();localStorage.setItem('deas.locale','en');});
afterEach(()=>{cleanup();vi.clearAllMocks();});
function Harness({initialLibrary=library,initialPositive='',initialNegative=''}:{initialLibrary?:WildcardLibrary|null;initialPositive?:string;initialNegative?:string}={}){
 const [rules,setRules]=useState(defaultWildcard);const [enabled,setEnabled]=useState(false);
 const [lib,setLib]=useState<WildcardLibrary|null>(initialLibrary);
 const [error,setError]=useState(initialLibrary?'':'404 missing plugin');
 return <I18nProvider><WildcardPanel library={lib} loading={false} error={error} onReload={()=>{setLib(library);setError('');}} value={rules} onChange={setRules} enabled={enabled} onEnabled={setEnabled} positive={initialPositive} negative={initialNegative} onInsert={name=>{void name;}}/><output data-testid="rules">{JSON.stringify(rules)}</output></I18nProvider>;
}
it('preserves independent preset/wildcard weights and explicit activation',async()=>{
 render(<Harness/>);
 fireEvent.click(await screen.findByText('Art'));
 fireEvent.click(screen.getByRole('checkbox',{name:'Lighting · Wildcards'}));
 fireEvent.change(screen.getByRole('spinbutton',{name:'Weight: Lighting · Wildcards'}),{target:{value:'1.2'}});
 fireEvent.click(screen.getByRole('checkbox',{name:'Lighting · Presets'}));
 expect(screen.getByText('Lighting · Wildcards ×1.2').parentElement?.textContent).toBe('Lighting · Wildcards ×1.2 {red|blue} light');
 expect(screen.getByText('Lighting · Presets').parentElement?.textContent).toBe('Lighting · Presets cinematic');
 fireEvent.click(screen.getByRole('checkbox',{name:'Active for generation'}));
 fireEvent.click(screen.getByRole('checkbox',{name:'New picks on every generation'}));
 const r=JSON.parse(screen.getByTestId('rules').textContent!);
 expect(r.selection_state['Art/Lighting/wildcards']).toEqual({enabled:true,weight:1.2});
 expect(r.selection_state['Art/Lighting/presets']).toEqual({enabled:true,weight:1});
 expect(r.auto_reroll).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'New picks'}));
 expect(JSON.parse(screen.getByTestId('rules').textContent!).selection_state).toEqual(r.selection_state);
});
it('shows a retryable library error',async()=>{
 render(<Harness initialLibrary={null}/>);
 expect(await screen.findByRole('alert')).toHaveProperty('textContent',expect.stringContaining('404'));
 fireEvent.click(screen.getByRole('button',{name:'Reload library'}));
 await screen.findByText('Art');
});
it('random selection chooses available subjects without changing rule modes',async()=>{
 render(<Harness/>);await screen.findByText('Art');
 fireEvent.click(screen.getByRole('button',{name:'Random select'}));
 await waitFor(()=>expect(Object.keys(JSON.parse(screen.getByTestId('rules').textContent!).selection_state).length).toBeGreaterThan(0));
 const r=JSON.parse(screen.getByTestId('rules').textContent!);
 expect(r.auto_reroll).toBe(false);expect(r.seed).toBe(0);
 fireEvent.click(screen.getByRole('button',{name:'Clear selection'}));
 expect(JSON.parse(screen.getByTestId('rules').textContent!).selection_state).toEqual({});
});
it('lists $variables from the prompt, pins win over the seed, unknown tokens are flagged',async()=>{
 render(<Harness initialPositive="a $Art cat at ${Art}-light, $unknown"/>);
 const rows=document.querySelectorAll('[data-wildcard-var]');
 expect(rows.length).toBe(3); // Art:0, Art:1, unknown
 const pins=await screen.findByLabelText(/Pin Art:0/);
 fireEvent.change(pins,{target:{value:'{red|blue} light'}});
 const r=JSON.parse(screen.getByTestId('rules').textContent!);
 expect(JSON.parse(r.variable_selections)).toEqual({'Art:0':'{red|blue} light'});
 expect(document.querySelector('[data-wildcard-var="positive:unknown:0"]')?.textContent).toContain('Unknown variable unknown');
});
