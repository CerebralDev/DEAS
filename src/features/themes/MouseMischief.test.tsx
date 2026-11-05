import {I18nProvider} from '../../i18n/I18nProvider';
import { act,fireEvent,render as plainRender,screen,cleanup } from '@testing-library/react';
import { afterEach,beforeEach,expect,test,vi } from 'vitest';
import { MouseMischief } from './MouseMischief';
import { moveMouse,rapidKnocks } from './mouseMotion';
const render=(ui:Parameters<typeof plainRender>[0])=>plainRender(ui,{wrapper:I18nProvider});

beforeEach(()=>{
 vi.useFakeTimers();document.documentElement.dataset.themeMouse='true';
 vi.stubGlobal('matchMedia',()=>({matches:false}));
});
afterEach(()=>{cleanup();vi.useRealTimers();vi.unstubAllGlobals();delete document.documentElement.dataset.themeMouse;delete document.documentElement.dataset.cheeseStolen;});
const knock=()=>{const door=screen.getByRole('button',{name:/Mouse door/});for(let i=0;i<4;i++)fireEvent.click(door);};
const phase=()=>document.querySelector('[data-mouse-house]')?.getAttribute('data-phase');

test('pursuit is bounded by elapsed time and never overshoots; knocks expire',()=>{
 expect(moveMouse({x:0,y:0},{x:300,y:400},1,100)).toEqual({x:60,y:80});
 expect(moveMouse({x:0,y:0},{x:3,y:4},1,100)).toEqual({x:3,y:4});
 expect(rapidKnocks([0,300,600],1700)).toEqual([600,1700]);
});
test('an eligible idle window gets a random visit after the minimum 45 seconds',()=>{
 vi.spyOn(Math,'random').mockReturnValue(0);vi.spyOn(document,'hasFocus').mockReturnValue(true);
 render(<MouseMischief mouse="data:image/png;base64,YQ==" cheese="data:image/png;base64,YQ=="/>);fireEvent(window,new MouseEvent('pointermove',{clientX:20,clientY:20}));
 act(()=>vi.advanceTimersByTime(44_999));expect(phase()).toBe('idle');
 act(()=>vi.advanceTimersByTime(10));expect(phase()).toBe('chasing');
});
test('four rapid knocks start a chase; evasion lasts ten seconds then enters cooldown',()=>{
 render(<MouseMischief mouse="data:image/png;base64,YQ==" cheese="data:image/png;base64,YQ=="/>);const door=screen.getByRole('button',{name:/Mouse door/});
 fireEvent.click(door);fireEvent.click(door);fireEvent.click(door);expect(phase()).toBe('idle');
 fireEvent.click(door);expect(phase()).toBe('chasing');
 act(()=>vi.advanceTimersByTime(9900));expect(phase()).toBe('chasing');
 act(()=>vi.advanceTimersByTime(200));expect(phase()).toBe('returning');
 act(()=>vi.advanceTimersByTime(4000));expect(phase()).toBe('idle');
 act(()=>vi.advanceTimersByTime(89_000));expect(phase()).toBe('idle');
 expect(document.documentElement.dataset.cheeseStolen).toBeUndefined();
});
test('catching borrows only the cheese cursor and always restores it',()=>{
 render(<MouseMischief mouse="data:image/png;base64,YQ==" cheese="data:image/png;base64,YQ=="/>);
 fireEvent(window,new MouseEvent('pointermove',{clientX:innerWidth-35,clientY:35}));
 knock();act(()=>vi.advanceTimersByTime(1100));expect(phase()).toBe('caught');
 expect(document.documentElement.dataset.cheeseStolen).toBe('true');
 act(()=>vi.advanceTimersByTime(4000));expect(phase()).toBe('idle');
 expect(document.documentElement.dataset.cheeseStolen).toBeUndefined();
});
test('switching away removes the mouse and restores the cursor immediately',async()=>{
 render(<MouseMischief mouse="data:image/png;base64,YQ==" cheese="data:image/png;base64,YQ=="/>);knock();expect(phase()).toBe('chasing');
 await act(async()=>{document.documentElement.dataset.themeMouse='false';});
 expect(document.querySelector('[data-cheese-mouse]')).toBeNull();
 expect(screen.queryByRole('button',{name:/Mouse door/})).toBeNull();
 expect(document.documentElement.dataset.cheeseStolen).toBeUndefined();
 await act(async()=>{document.documentElement.dataset.themeMouse='true';});expect(phase()).toBe('idle');
});
test('hiding the window cancels a heist and restores the cheese decoration',()=>{
 render(<MouseMischief mouse="data:image/png;base64,YQ==" cheese="data:image/png;base64,YQ=="/>);
 fireEvent(window,new MouseEvent('pointermove',{clientX:innerWidth-35,clientY:35}));
 knock();act(()=>vi.advanceTimersByTime(1100));expect(phase()).toBe('caught');
 vi.spyOn(document,'hidden','get').mockReturnValue(true);
 fireEvent(document,new Event('visibilitychange'));expect(phase()).toBe('idle');
 expect(document.documentElement.dataset.cheeseStolen).toBeUndefined();
});
