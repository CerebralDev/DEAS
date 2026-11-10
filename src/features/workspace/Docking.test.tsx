import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { useEffect, useState } from 'react';
import { I18nProvider } from '../../i18n/I18nProvider';
import { RetainedPane, WorkspaceTooltips } from './Docking';
afterEach(cleanup);


it('moves a single mounted pane without losing draft or session',()=>{
 const left=document.createElement('div'),right=document.createElement('div');document.body.append(left,right);let mounted=0;
 function Session(){const [draft,setDraft]=useState('');useEffect(()=>{mounted++;},[]);return <input aria-label="draft" value={draft} onChange={e=>setDraft(e.target.value)}/>;}
 const view=render(<RetainedPane target={left}><Session/></RetainedPane>);fireEvent.change(screen.getByLabelText('draft'),{target:{value:'unsent'}});
 const input=screen.getByLabelText('draft');view.rerender(<RetainedPane target={right}><Session/></RetainedPane>);
 expect(right.contains(input)).toBe(true);expect(input).toHaveProperty('value','unsent');expect(mounted).toBe(1);view.unmount();left.remove();right.remove();
});
it('focus exposes help and Escape dismisses it',()=>{render(<I18nProvider><WorkspaceTooltips/><button title="Only reorganizes virtual folders">Help</button></I18nProvider>);fireEvent.focusIn(screen.getByText('Help'));expect(screen.getByRole('tooltip').textContent).toBe('Only reorganizes virtual folders');fireEvent.keyDown(document,{key:'Escape'});expect(screen.queryByRole('tooltip')).toBeNull();});
