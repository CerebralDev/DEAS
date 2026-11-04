import test from 'node:test';
import assert from 'node:assert/strict';
import { initialSetupState, storageTarget, transitionSetup, setupPlan, validateBackend, validateStorage, activeSetupStep, type SetupState } from '../src/lib/setup.ts';
const select=(s:SetupState,step:'storage'|'acceleration',value:string)=>transitionSetup(s,{type:'select',step,value});
function automatic() {
 let s=transitionSetup(initialSetupState(),{type:'automatic'});
 return select(select(s,'storage','/data/DEAS'),'acceleration','cpu');
}
test('automatic review keeps all fields editable without per-step confirmation',()=>{
 let s=transitionSetup(initialSetupState(),{type:'automatic'});
 assert.equal(activeSetupStep(s),'review');assert.equal(setupPlan(s),null);assert.deepEqual(s.features,['director']);
 s=transitionSetup(s,{type:'feature',id:'director'});assert.deepEqual(s.features,[]);
 s=select(s,'storage','/data/new');s=select(s,'acceleration','cpu');
 assert.ok(setupPlan(s));
 s=transitionSetup(s,{type:'option',id:'ffmpeg',enabled:true});
 s=select(s,'storage','/data/other');
 assert.equal(s.options.ffmpeg,true);assert.equal(activeSetupStep(s),'review');
 s=select(s,'storage','');assert.equal(setupPlan(s),null);
});
test('Director is the sole selectable package and expands the canonical install bundle',()=>{
 let s=automatic();
 assert.equal(transitionSetup(s,{type:'feature',id:'forge'}),s);
 const plan=setupPlan(s);assert.ok(plan);assert.deepEqual(plan.features,['director','forge','refmods','continuity']);
 s=transitionSetup(s,{type:'feature',id:'director'});
 assert.deepEqual(setupPlan(s)?.features,[]);
 s=transitionSetup(s,{type:'feature',id:'director'});
 assert.deepEqual(setupPlan(s)?.features,['director','forge','refmods','continuity']);
});
test('core only is valid without optional packages or generation claims',()=>{
 const s=transitionSetup(automatic(),{type:'feature',id:'director'});
 assert.deepEqual(setupPlan(s),{version:2,status:'NOT INSTALLED',route:'managed',storagePath:'/data/DEAS',backend:'cpu',features:[],options:{sage:false,radial:false,flash:false,ffmpeg:false},comfyRef:'master',provisioningAvailable:false});
});
test('automatic backend and storage edits preserve explicit options and source ref',()=>{
 let s=automatic();
 for(const id of ['sage','radial','flash','ffmpeg'] as const)s=transitionSetup(s,{type:'option',id,enabled:true});
 s=transitionSetup(s,{type:'ref',value:'v0.3.50'});
 for(const backend of ['nvidia','amd','intel','cpu']) {
  s=select(s,'acceleration',backend);s=select(s,'storage','/data/other');
  const plan=setupPlan(s);assert.ok(plan);assert.equal(plan.backend,backend);assert.equal(plan.comfyRef,'v0.3.50');assert.ok(Object.values(plan.options).every(Boolean));
 }
 assert.equal(select(s,'acceleration','unknown'),s);
 const plan=setupPlan(s)!;plan.options.sage=false;assert.equal(s.options.sage,true);
});
test('automatic planning rejects invalid storage, refs and legacy existing-route states',()=>{
 assert.equal(setupPlan(initialSetupState()),null);
 const s=automatic();
 for(const value of ['','relative','/data/../etc'])assert.equal(setupPlan(select(s,'storage',value)),null);
 for(const value of ['','bad ref','x;command'])assert.equal(setupPlan(transitionSetup(s,{type:'ref',value})),null);
 assert.equal(setupPlan({...s,values:{route:'existing',backend:'https://localhost:8188/comfy'}}),null);
});
test('validates URLs and absolute storage',()=>{
 for(const v of ['', 'localhost:8188','ftp://localhost','https://user:pass@localhost','https://localhost?','https://localhost#','https://localhost/ space'])assert.equal(validateBackend(v),false,v);
 for(const v of ['', '~/.deas','relative/path','/','/data/../etc','C:\\','/data\nname'])assert.equal(validateStorage(v),false,v);
 for(const v of ['/data/deas','C:\\DEAS','D:/Models'])assert.equal(validateStorage(v),true,v);
});
test('native folder joining preserves Linux and Windows parent spelling and rejects unsafe child names',()=>{
 assert.equal(storageTarget('/data','deas-environment'),'/data/deas-environment');
 assert.equal(storageTarget('/','new'),'/new');
 assert.equal(storageTarget('C:\\Models','new'),'C:\\Models\\new');
 assert.equal(storageTarget('D:/Models/','new'),'D:/Models/new');
 assert.equal(storageTarget('C:\\','new'),'C:\\new');
 for(const child of ['','..','../escape','a/b','a\\b','CON','x.',' x','bad:name'])assert.equal(storageTarget('/data',child),null,child);
});
