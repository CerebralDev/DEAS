// Tauri injects this in subframes too; never give the editor native IPC access.
(() => {
 if(window.parent===window||location.protocol!=='http:'||location.hostname!=='127.0.0.1')return;
 const allowed=new Set(['tauri://localhost','http://tauri.localhost','https://tauri.localhost']);
 // Development origin is supplied only by debug builds.
 if(typeof DEAS_DEV_ORIGIN!=='undefined')allowed.add(DEAS_DEV_ORIGIN);
 // WebKitGTK composites backdrop-filter in software: ComfyUI's blurred dialog masks over animated thumbnails (Templates) pin the compositor at 100% and freeze the app.
 document.addEventListener('DOMContentLoaded',()=>document.head.append(Object.assign(document.createElement('style'),{textContent:'*,*::before,*::after{-webkit-backdrop-filter:none!important;backdrop-filter:none!important}'})));
 // WebKitGTK rasterizes a composited transform:scale() layer at 1x and stretches it, so ComfyUI's DOM widgets (prompt textareas) blur with canvas zoom.
 // Same geometry via CSS zoom is re-laid-out and painted at the real size; zoom also multiplies left/top, hence the division.
 const sharpen=el=>{
  const m=/^scale\(([^)]+)\)$/.exec(el.style.transform),z=m?Number(m[1]):el.deasZoom;
  if(!(z>0))return;
  if(m){el.style.transform='none';el.style.zoom=String(z);el.deasZoom=z;}
  const raw=el.deasRaw??={},out=el.deasOut??={};
  for(const k of ['left','top']){
   // Vue only rewrites left/top when they change, so a value differing from ours is a fresh unscaled one.
   if(el.style[k]!==out[k])raw[k]=parseFloat(el.style[k]);else if(!m)continue;
   if(Number.isFinite(raw[k])){el.style[k]=raw[k]/z+'px';out[k]=el.style[k];}
  }
 };
 new MutationObserver(list=>{for(const r of list)if(r.target.classList?.contains('dom-widget'))sharpen(r.target);})
  .observe(document,{subtree:true,attributes:true,attributeFilter:['style']});
 let loading=false;
 window.addEventListener('message',async event=>{
  const data=event.data;
  if(event.source!==window.parent||!allowed.has(event.origin)||!data||typeof data.id!=='string'||!['deas:probe','deas:load'].includes(data.type))return;
  const reply=type=>event.source.postMessage({type,id:data.id},event.origin);
  try{
   // Frontend 1.57+ no longer exposes comfyAPI.app, so its /scripts/app.js shim throws; the live app is window.app.
   const app=window.app?.canvas?window.app:(await import(new URL('/scripts/app.js',location.origin).href)).app;
   if(!app?.canvas)return;
   if(data.type==='deas:probe'){reply('deas:ready');return;}
   const w=data.workflow;
   if(loading||!w||!Array.isArray(w.nodes)||!Array.isArray(w.links)||!w.nodes.every(n=>n&&typeof n.type==='string'&&['string','number'].includes(typeof n.id))||JSON.stringify(w).length>2*1024*1024||typeof data.name!=='string')throw Error('Invalid workflow');
   loading=true;
   try{
    // A named import opens a separate workflow, preserving the current editor draft.
    const loaded=await app.loadGraphData(w,true,true,`DEAS ${data.name} (${data.id})`);
    reply(loaded===false?'deas:error':'deas:loaded');
   }finally{loading=false;}
  }catch{reply('deas:error');}
 });
})();
