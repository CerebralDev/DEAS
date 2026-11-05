import {I18nProvider} from '../../i18n/I18nProvider';
import { render as plainRender,screen } from '@testing-library/react';
import { expect,test } from 'vitest';
import { FrenchWineProgress } from './FrenchWineProgress';
const render=(ui:Parameters<typeof plainRender>[0])=>plainRender(ui,{wrapper:I18nProvider});

test('the wine drains with native progress and has no invented percentage while waiting',()=>{
 const props={active:true,label:'Generating',wineFull:'data:image/png;base64,YQ==',wineEmpty:'data:image/png;base64,YQ=='};
 const view=render(<FrenchWineProgress {...props}/>);
 const indicator=screen.getByRole('progressbar');
 expect(indicator.hasAttribute('aria-valuenow')).toBe(false);
 for(const [value,height] of [[0,10],[25,7.5],[75,2.5],[100,0]]){
  view.rerender(<FrenchWineProgress {...props} progress={{value,max:100}}/>);
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe(String(value));
  expect(view.container.querySelector('[data-wine-liquid]')?.getAttribute('height')).toBe(String(height));
 }
 view.rerender(<FrenchWineProgress {...props} progress={{value:5,max:0}}/>);
 expect(screen.getByRole('progressbar').hasAttribute('aria-valuenow')).toBe(false);
 view.rerender(<FrenchWineProgress {...props} active={false}/>);
 expect(screen.queryByRole('progressbar')).toBeNull();
});
