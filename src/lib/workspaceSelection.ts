/** Optional presentation-state feed. No inference, persistence, or Chat import. */
const selected:Record<string,Record<string,string>>={};
export function selectContext(area:string,values:Record<string,string>){selected[area]={...values};window.dispatchEvent(new CustomEvent('deas:workspace-context',{detail:area}));}

export function readSelectedContext(area:string){return selected[area]??{};}
