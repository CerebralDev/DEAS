import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {afterEach,expect,test,vi} from 'vitest';
import {I18nProvider} from '../../i18n/I18nProvider';
import {Sam3Controls,samTargets} from './Sam3Controls';
afterEach(cleanup);
const value={checkpoint:'sam3.1.safetensors',targets:'eyes, feet; hands',denoise:0.25,threshold:0.5,enabled:false};
test('target groups retain order and plural matches stay in their group',()=>{
 expect(samTargets(' eyes, feet; hands\nred shoes,, ')).toEqual(['eyes','feet','hands','red shoes']);
});
test('SAM settings never execute until an explicit action and missing SAM does not block painted-mask refinement',()=>{
 localStorage.setItem('deas.locale.v1','en');const onChange=vi.fn(),onMask=vi.fn(),onRefine=vi.fn();
 const view=render(<I18nProvider><Sam3Controls value={value} onChange={onChange} models={[value.checkpoint]} margin={32} onMargin={vi.fn()} busy={false} inpaint onMask={onMask} onRefine={onRefine} canRefine/></I18nProvider>);
 expect(onMask).not.toHaveBeenCalled();expect(onRefine).not.toHaveBeenCalled();
 const denoise=view.container.querySelector('[data-sam="denoise"]')!;
 fireEvent.change(denoise,{target:{value:'0.4'}});expect(onChange).toHaveBeenLastCalledWith({...value,denoise:0.4});
 fireEvent.click(view.container.querySelector('[data-sam="mask"]')!);expect(onMask).toHaveBeenCalledOnce();
 view.rerender(<I18nProvider><Sam3Controls value={{...value,targets:''}} onChange={onChange} models={[]} margin={32} onMargin={vi.fn()} unavailable="Missing SAM checkpoint" busy={false} inpaint onMask={onMask} onRefine={onRefine} canRefine/></I18nProvider>);
 expect(view.container.querySelector('[data-sam="mask"]')).toHaveProperty('disabled',true);
 expect(view.container.querySelector('[data-sam="refine"]')).toHaveProperty('disabled',false);
 fireEvent.click(view.container.querySelector('[data-sam="refine"]')!);expect(onRefine).toHaveBeenCalledOnce();
 expect(screen.getByRole('checkbox')).toHaveProperty('checked',false);
});
