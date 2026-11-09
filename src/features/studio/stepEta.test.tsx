import {expect,test} from 'vitest';
import {formatEta,observeStep,stepRemaining,type StepEta} from './stepEta';

const feed=(steps:[number,number,number][],start?:StepEta)=>steps.reduce<StepEta|undefined>((eta,[value,max,at])=>observeStep(eta,{value,max},at),start)!;

test('warm-up: no estimate before two step updates',()=>{
 expect(stepRemaining(feed([[0,10,0]]),0)).toBeNull();
 expect(stepRemaining(feed([[0,10,0],[1,10,1000]]),1000)).toBeNull();
 expect(stepRemaining(feed([[0,10,0],[1,10,1000],[2,10,2000]]),2000)).toBe(8000);
});
test('a repeated value keeps timing and the estimate counts down, clamped at zero and hidden when stale',()=>{
 const eta=feed([[0,10,0],[1,10,1000],[2,10,2000],[2,10,2500]]);
 expect(stepRemaining(eta,2500)).toBe(7500);
 expect(stepRemaining(eta,11000)).toBe(0);
 expect(stepRemaining(eta,40000)).toBeNull();
});
test('rate change moves the moving average toward the new step time',()=>{
 const eta=feed([[0,10,0],[1,10,1000],[2,10,2000],[3,10,5000]]);
 expect(eta.msPerStep).toBe(1600);expect(stepRemaining(eta,5000)).toBe(7*1600);
});
test('a new phase (value drops or max changes) restarts the estimate',()=>{
 const warm=feed([[0,10,0],[1,10,1000],[2,10,2000]]);
 expect(stepRemaining(observeStep(warm,{value:1,max:10},3000),3000)).toBeNull();
 expect(stepRemaining(observeStep(warm,{value:3,max:20},3000),3000)).toBeNull();
 expect(stepRemaining(feed([[10,10,9000]],warm),9000)).toBeNull();
});
test('formats seconds and minutes for the locale',()=>{
 expect(formatEta(44200,'en')).toBe('45 sec');
 expect(formatEta(185000,'en')).toBe('3 min');
});
