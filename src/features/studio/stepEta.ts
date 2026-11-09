/** Client-side remaining-time estimate from observed sampler step updates (an estimate, never native timing). */
export interface StepEta {max:number;value:number;at:number;msPerStep:number|null;updates:number}

/** A drop in value or a new max is a new sampling phase (refine, second pass, SAM): restart the estimate. */
export function observeStep(prev:StepEta|undefined,progress:{value:number;max:number},now:number):StepEta {
 const {value,max}=progress;
 if(!prev||max!==prev.max||value<prev.value)return {max,value,at:now,msPerStep:null,updates:0};
 if(value===prev.value)return prev;
 const per=(now-prev.at)/(value-prev.value);
 return {max,value,at:now,msPerStep:prev.msPerStep===null?per:prev.msPerStep*.7+per*.3,updates:prev.updates+1};
}

/** Milliseconds left, or null during warm-up (<2 step updates), after the last step, or when updates went stale. */
export function stepRemaining(eta:StepEta|undefined,now:number):number|null {
 if(!eta||eta.msPerStep===null||eta.updates<2||eta.value>=eta.max)return null;
 const since=now-eta.at;
 if(since>Math.max(30000,eta.msPerStep*5))return null;
 return Math.max(0,(eta.max-eta.value)*eta.msPerStep-since);
}

export function formatEta(ms:number,locale:string):string {
 const s=Math.ceil(ms/1000);
 return new Intl.NumberFormat(locale,{style:'unit',unit:s<60?'second':'minute',unitDisplay:'short',maximumFractionDigits:0}).format(s<60?s:Math.round(s/60));
}
