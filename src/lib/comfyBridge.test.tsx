import bridge from '../../src-tauri/src/comfy_bridge.js?raw';
import { expect, test, vi } from 'vitest';
// Mock only the module import; execute the shipped all-frame receiver unchanged otherwise.
const source=bridge.replace("import(new URL('/scripts/app.js',location.origin).href)",'importApp()');
const workflow={nodes:[{id:1,type:'CLIPTextEncode'}],links:[]};
function receiver(top=false,hostname='127.0.0.1'){
 let listener:((event:Record<string,unknown>)=>Promise<void>)|undefined;
 const parent={postMessage:vi.fn()},app={canvas:{},loadGraphData:vi.fn().mockResolvedValue(true)},importApp=vi.fn(async()=>({app}));
 const window={parent,addEventListener:vi.fn((_name:string,fn:typeof listener)=>listener=fn)};
 new Function('window','location','importApp',(top?'window.parent=window;':'')+source)(window,{protocol:'http:',hostname,origin:`http://${hostname}:1234`},importApp);
 const send=(data:unknown,origin='tauri://localhost',source:unknown=parent)=>listener?.({source,origin,data});
 return {parent,app,importApp,send,hasListener:()=>!!listener};
}
test('bridge refuses top-level/non-loopback frames and foreign message senders',async()=>{
 expect(receiver(true).hasListener()).toBe(false);
 expect(receiver(false,'example.com').hasListener()).toBe(false);
 const r=receiver();await r.send({type:'deas:probe',id:'a'},'https://evil.invalid');await r.send({type:'deas:probe',id:'a'},'tauri://localhost',{});
 expect(r.importApp).not.toHaveBeenCalled();expect(r.parent.postMessage).not.toHaveBeenCalled();
});
test('bridge validates graphs, rejects overlapping loads and reports load failures',async()=>{
 const r=receiver();
 for(const w of [null,{}, {nodes:[null],links:[]},{nodes:[{type:'x'}],links:[]},{...workflow,extra:'x'.repeat(2*1024*1024)}])await r.send({type:'deas:load',id:'bad',workflow:w,name:'a.png'});
 expect(r.app.loadGraphData).not.toHaveBeenCalled();
 expect(r.parent.postMessage).toHaveBeenLastCalledWith({type:'deas:error',id:'bad'},'tauri://localhost');
 let finish!:(v:boolean)=>void;r.app.loadGraphData.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
 const first=r.send({type:'deas:load',id:'one',workflow,name:'a.png'});await Promise.resolve();
 await r.send({type:'deas:load',id:'two',workflow,name:'a.png'});expect(r.app.loadGraphData).toHaveBeenCalledTimes(1);
 finish(true);await first;
 r.app.loadGraphData.mockResolvedValueOnce(false);await r.send({type:'deas:load',id:'three',workflow,name:'a.png'});
 expect(r.parent.postMessage).toHaveBeenLastCalledWith({type:'deas:error',id:'three'},'tauri://localhost');
 r.app.loadGraphData.mockRejectedValueOnce(new Error('failed'));await r.send({type:'deas:load',id:'four',workflow,name:'a.png'});
 expect(r.parent.postMessage).toHaveBeenLastCalledWith({type:'deas:error',id:'four'},'tauri://localhost');
});
test('bridge disables backdrop blur inside the Comfy frame',()=>{
 receiver();document.dispatchEvent(new Event('DOMContentLoaded'));
 expect([...document.head.querySelectorAll('style')].some(s=>s.textContent?.includes('backdrop-filter:none!important'))).toBe(true);
});
test('each acknowledged import uses a distinct workflow name even for the same media',async()=>{
 const r=receiver();await r.send({type:'deas:probe',id:'one'});
 expect(r.parent.postMessage).toHaveBeenLastCalledWith({type:'deas:ready',id:'one'},'tauri://localhost');
 for(const id of ['one','two'])await r.send({type:'deas:load',id,workflow,name:'a.png'});
 expect(r.app.loadGraphData.mock.calls.map(c=>c[3])).toEqual(['DEAS a.png (one)','DEAS a.png (two)']);
 expect(r.parent.postMessage).toHaveBeenLastCalledWith({type:'deas:loaded',id:'two'},'tauri://localhost');
});
test('bridge uses the live window.app on frontends whose /scripts/app.js shim no longer exports it',async()=>{
 let listener:((event:Record<string,unknown>)=>Promise<void>)|undefined;
 const parent={postMessage:vi.fn()},app={canvas:{},loadGraphData:vi.fn().mockResolvedValue({})},importApp=vi.fn(async()=>{throw new TypeError("Cannot read properties of undefined (reading 'app')");});
 const window={parent,app,addEventListener:vi.fn((_name:string,fn:typeof listener)=>listener=fn)};
 new Function('window','location','importApp',source)(window,{protocol:'http:',hostname:'127.0.0.1',origin:'http://127.0.0.1:1234'},importApp);
 await listener?.({source:parent,origin:'tauri://localhost',data:{type:'deas:probe',id:'p'}});
 expect(parent.postMessage).toHaveBeenLastCalledWith({type:'deas:ready',id:'p'},'tauri://localhost');
 await listener?.({source:parent,origin:'tauri://localhost',data:{type:'deas:load',id:'l',workflow,name:'a.png'}});
 expect(parent.postMessage).toHaveBeenLastCalledWith({type:'deas:loaded',id:'l'},'tauri://localhost');
 expect(importApp).not.toHaveBeenCalled();
});
