import {useEffect,useLayoutEffect,useImperativeHandle,useRef,useState,type Ref} from 'react';
import {useI18n} from '../../i18n/I18nProvider';
import {oneOf,usePref} from '../../lib/preferences';
import {sourcePoint,zoomAt,stampStroke,maskBounds,floodRegion,flattenImages,hasPaint,uncoveredCells,type ImageLayer,type Point,type View} from './imageEditor';
import {MEDIA_DRAG,draggedItem,type MediaItem} from '../../lib/media';
import './imageEditor.css';
export interface EditorSnapshot{source:Props['source'];masks:Mask[];base:ImageLayer;baseIndex:number;nextMask:number;active:string}
export type SamSelection={positivePoints?:Point[];negativePoints?:Point[];box?:{x:number;y:number;width:number;height:number}};
export interface EditorHandle{contentRevision:()=>number;capture:()=>EditorSnapshot;imagePng:()=>Promise<Uint8Array|null>;sourcePng:()=>Promise<Uint8Array>;imageEdited:()=>boolean;maskPng:()=>Promise<Uint8Array>;maskBounds:(padding:number)=>{x:number;y:number;width:number;height:number};addMask:(png:Uint8Array,name?:string)=>Promise<boolean>;refineMasks:()=>Promise<{name:string;png:Uint8Array;bounds:{x:number;y:number;width:number;height:number}}[]>}
type Mask={id:string;n:number;name?:string;visible:boolean;canvas:HTMLCanvasElement;content:boolean};
export type Props={ref?:Ref<EditorHandle>;source:{src:string;name:string;width:number;height:number};layers:ImageLayer[];onLayers:(layers:ImageLayer[])=>void;maskMode:boolean;onMask:(present:boolean)=>void;/** A dock tile dropped onto the picture, with the drop point in source pixels. */onInsert:(item:MediaItem,at:Point)=>void;/** A change (e.g. entering side by side) refits the picture even after zooming. */layout?:string;/** In-memory authoring state, only accepted for this exact source. */snapshot?:EditorSnapshot;draftGeneration?:string|number;sam?:{busy:boolean;unavailable?:string;onPick:(selection:SamSelection)=>void}};
const diameterOK=(v:unknown):v is number=>typeof v==='number'&&Number.isInteger(v)&&v>=1&&v<=1024;
// Mask overlay snaps to common opacities when dragged within 3 % of them; other values stay free.
const overlayStops=[10,25,50,75];
const LAYER_DRAG='application/x-deas-editor-layer';
const snapOverlay=(n:number)=>overlayStops.find(v=>Math.abs(v-n)<=3)??n;
const toleranceOK=(v:unknown):v is number=>typeof v==='number'&&Number.isInteger(v)&&v>=0&&v<=255;
const opacityOK=(v:unknown):v is number=>typeof v==='number'&&Number.isInteger(v)&&v>=0&&v<=100;
const grayOK=(v:unknown):v is number=>typeof v==='number'&&Number.isInteger(v)&&v>=0&&v<=255;
const hexOK=(v:unknown):v is string=>typeof v==='string'&&/^#[0-9a-f]{6}$/i.test(v);
const palette=['#000000','#ffffff','#808080','#ff3b30','#ff9500','#ffcc00','#34c759','#00c7be','#007aff','#5856d6','#af52de','#8e5a3a'];
/** Mounts a live canvas the strokes draw into directly (no per-stroke copy). */
function Mounted({node}:{node:HTMLCanvasElement}){const ref=useRef<HTMLSpanElement>(null);useEffect(()=>{ref.current?.replaceChildren(node);},[node]);return <span ref={ref} className="sie-paint" aria-hidden="true"/>;}
const icons={select:'M5 3l13 7-6 1.5L9 18z',brush:'M19 3l2 2-9 9-2-2zM9 13c-3 0-4 2-4 4s-1 3-3 3c3 1 8 1 9-4',erase:'M15 4l5 5-10 10H6l-3-3zM8.5 10.5l5 5M11 19h10',pan:'M12 3v18M3 12h18M9 6l3-3 3 3M9 18l3 3 3-3M6 9l-3 3 3 3M18 9l3 3-3 3',samPoint:'M12 3v6M12 15v6M3 12h6M15 12h6M12 10a2 2 0 1 0 0 4 2 2 0 0 0 0-4',samBox:'M4 4h16v16H4z',wand:'M4 20L15 9M17 3v3M15.5 4.5h3M20 8l1 1M12 4l1 1M19 12h2',eye:'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z',eyeOff:'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM4 4l16 16'};
const Icon=({d}:{d:string})=><svg viewBox="0 0 24 24" aria-hidden="true"><path d={d}/></svg>;
function copyCanvas(source:HTMLCanvasElement){const canvas=document.createElement('canvas');canvas.width=source.width;canvas.height=source.height;const ctx=canvas.getContext('2d');if(!ctx)throw new Error('Canvas unavailable');ctx.drawImage(source,0,0);return canvas;}
export const copyImageLayer=(layer:ImageLayer):ImageLayer=>({...layer,...(layer.paint?{paint:copyCanvas(layer.paint)}:{})});
function newMask(n:number,width:number,height:number):Mask{
  const canvas=document.createElement('canvas');canvas.width=width;canvas.height=height;
  return {id:crypto.randomUUID(),n,visible:true,canvas,content:false};
}
function Thumbnail({mask,revision}:{mask:Mask;revision:number}){
  const ref=useRef<HTMLCanvasElement>(null);
  useEffect(()=>{const c=ref.current,ctx=c?.getContext('2d');if(c&&ctx){ctx.clearRect(0,0,c.width,c.height);ctx.drawImage(mask.canvas,0,0,c.width,c.height);}},[mask,revision]);
  return <canvas ref={ref} width={44} height={36} aria-hidden="true"/>;
}
export function StudioImageEditor({ref,source,layers,onLayers,maskMode,onMask,onInsert,layout,snapshot,draftGeneration,sam}:Props){
  const {t}=useI18n(),label=(key:string)=>t(`studio.editor.${key}`);
  const viewport=useRef<HTMLDivElement>(null),world=useRef<HTMLDivElement>(null),overlay=useRef<HTMLCanvasElement>(null);
  const cursor=useRef<HTMLDivElement>(null);
  const operation=useRef(0),contentRevision=useRef(0);
  useLayoutEffect(()=>{operation.current++;return()=>{operation.current++;};},[draftGeneration,source.src,source.width,source.height]);
  const [view,setView]=useState<View>({scale:1,x:0,y:0});
  const [tool,setTool]=useState<'select'|'brush'|'erase'|'wand'|'samPoint'|'samBox'|'pan'>(maskMode?'brush':'select');
  const [tolerance,setTolerance]=usePref('studio.editor.wandTolerance',32,toleranceOK);
  // Source pixels for the magic wand, read once per picture through a blob (an asset:// image would taint the canvas).
  const pixels=useRef<{src:string;data:Promise<ImageData>}|null>(null);
  const [diameter,setDiameter]=usePref('studio.run.brush',48,diameterOK);
  const [shape,setShape]=usePref('studio.editor.shape','round',oneOf('round','square'));
  const [opacity,setOpacity]=usePref('studio.editor.overlay',75,opacityOK);
  const [color,setColor]=usePref('studio.editor.color','#ff3b30',hexOK);const [hex,setHex]=useState(color);useEffect(()=>setHex(color),[color]);
  const [gray,setGray]=usePref('studio.editor.maskValue',255,grayOK);
  const saved=snapshot&&snapshot.source.src===source.src&&snapshot.source.width===source.width&&snapshot.source.height===source.height?snapshot:undefined;
  const [masks,setMasks]=useState<Mask[]>(()=>saved?.masks.map(m=>({...m,canvas:copyCanvas(m.canvas)}))??[newMask(1,source.width,source.height)]);
  const nextMask=useRef(saved?.nextMask??2),[active,setActive]=useState(saved?.active??'mask:1'),[revision,setRevision]=useState(0),[error,setError]=useState('');
  const masksRef=useRef(masks);masksRef.current=masks;
  const samPoints=useRef<{positivePoints:Point[];negativePoints:Point[]}>({positivePoints:[],negativePoints:[]});
  const [samRect,setSamRect]=useState<SamSelection['box']>();
  useLayoutEffect(()=>{samPoints.current={positivePoints:[],negativePoints:[]};gesture.current=null;setSamRect(undefined);},[tool,draftGeneration,source.src,source.width,source.height]);
  const gesture=useRef<null|{pointer:number;kind:'samBox';start:Point}|{pointer:number;kind:'paint';ctx:CanvasRenderingContext2D;mask?:Mask;map:(p:Point)=>Point;k:number;last:Point;erase:boolean;diameter:number;shape:'round'|'square';color:string}|{pointer:number;kind:'pan';start:Point;view:View}|{pointer:number;kind:'move'|'resize';start:Point;layer:ImageLayer}>(null);
  const [base,setBase]=useState<ImageLayer>(()=>saved?copyImageLayer(saved.base):({id:'base',name:source.name,src:source.src,x:0,y:0,width:source.width,height:source.height,visible:true}));
  // A native relocation changes the location, not this source's paint/masks/geometry.
  useEffect(()=>{setBase(value=>value.src===source.src&&value.name===source.name?value:{...value,src:source.src,name:source.name});},[source.src,source.name]);
  const [baseIndex,setBaseIndex]=useState(saved?.baseIndex??0);
  const images=[...layers];images.splice(Math.min(baseIndex,images.length),0,base);
  const updateImages=(next:ImageLayer[])=>{contentRevision.current++;const i=next.findIndex(l=>l.id==='base');setBase(next[i]??{...base,visible:false});setBaseIndex(Math.max(0,i));onLayers(next.filter(l=>l.id!=='base'));};
  const imageEdited=()=>!base.visible||base.x!==0||base.y!==0||base.width!==source.width||base.height!==source.height||baseIndex>0||hasPaint(base.paint)||layers.some(l=>l.x<0||l.y<0);
  // Inpaint: frame (source size) areas no visible picture covers are outpainted.
  const outpaint=maskMode?uncoveredCells(images.filter(l=>l.visible),source.width,source.height):[];
  const outpaintRef=useRef(outpaint.length>0);outpaintRef.current=outpaint.length>0;
  const report=(painted:boolean)=>onMask(painted||outpaintRef.current);
  const draggedLayer=useRef<string|null>(null);
  const activeMask=masks.find(m=>`mask:${m.n}`===active);
  const activeImage=images.find(l=>l.id===active);
  // All pictures, including the base, use the same brush and transform path.
  const brushEnabled=(maskMode&&!!activeMask?.visible)||!!activeImage?.visible;
  const painting=tool==='brush'||tool==='erase';
  const brushWheel=useRef<{diameter:number;set:(n:number)=>void}|null>(null);brushWheel.current=painting&&brushEnabled?{diameter,set:setDiameter}:null;
  function track(e:React.PointerEvent){const c=cursor.current,r=viewport.current?.getBoundingClientRect();if(c&&r){c.style.left=`${e.clientX-r.left}px`;c.style.top=`${e.clientY-r.top}px`;c.hidden=false;}}
  function fail(){setError(label('canvasError'));onMask(false);}
  function redraw(changed=true){
    if(changed)contentRevision.current++;
    const c=overlay.current,ctx=c?.getContext('2d');if(!c||!ctx){fail();return;}
    ctx.clearRect(0,0,c.width,c.height);
    for(const m of masksRef.current)if(m.visible)ctx.drawImage(m.canvas,0,0);
    setRevision(r=>r+1);
  }
  function finish(){
    const g=gesture.current;gesture.current=null;
    if(g?.kind==='samBox')setSamRect(undefined);
    if(g?.kind==='paint'&&g.mask)settle(g.mask);
  }
  /** Re-reads whether a mask holds any pixels and repaints the overlay. */
  function settle(mask:Mask){
    try{const ctx=mask.canvas.getContext('2d');if(!ctx)throw Error();
      const data=ctx.getImageData(0,0,source.width,source.height).data;
      mask.content=false;for(let i=3;i<data.length;i+=4)if(data[i]){mask.content=true;break;}
      report(masksRef.current.some(m=>m.visible&&m.content));redraw();
    }catch{fail();}
  }
  function sourcePixels(){
    if(pixels.current?.src!==source.src)pixels.current={src:source.src,data:fetch(source.src).then(r=>r.blob()).then(createImageBitmap).then(bitmap=>{
      const c=document.createElement('canvas');c.width=source.width;c.height=source.height;const ctx=c.getContext('2d');if(!ctx)throw Error('canvas');
      ctx.drawImage(bitmap,0,0,source.width,source.height);bitmap.close();return ctx.getImageData(0,0,source.width,source.height);})};
    return pixels.current.data;
  }
  /** Magic wand: the connected area of similar colour around the click joins the active mask (Alt removes it). */
  async function wand(p:Point,remove:boolean){
    const mask=activeMask;if(!mask||!mask.visible)return;
    const x=Math.floor(p.x),y=Math.floor(p.y);if(x<0||y<0||x>=source.width||y>=source.height)return;
    const version=operation.current;
    try{
      const img=await sourcePixels();if(operation.current!==version)return;const region=floodRegion(img.data,img.width,img.height,x,y,tolerance);
      const fill=new ImageData(img.width,img.height);
      for(let i=0;i<region.length;i++)if(region[i]){const o=i*4;fill.data[o]=fill.data[o+1]=fill.data[o+2]=gray;fill.data[o+3]=255;}
      const stamp=document.createElement('canvas');stamp.width=img.width;stamp.height=img.height;stamp.getContext('2d')!.putImageData(fill,0,0);
      const ctx=mask.canvas.getContext('2d');if(!ctx)throw Error('canvas');
      ctx.save();ctx.globalCompositeOperation=remove?'destination-out':'source-over';ctx.drawImage(stamp,0,0);ctx.restore();
      settle(mask);
    }catch{if(operation.current===version){pixels.current=null;fail();}}
  }
  // Stays fitted across viewport resizes (e.g. entering side-by-side compare) until the user zooms or pans.
  const auto=useRef(true);
  function fit(){const r=viewport.current?.getBoundingClientRect();if(r&&r.width>0&&r.height>0){auto.current=true;const scale=Math.min(r.width/source.width,r.height/source.height);setView({scale,x:(r.width-source.width*scale)/2,y:(r.height-source.height*scale)/2});}}
  useEffect(()=>{fit();const el=viewport.current;if(!el||typeof ResizeObserver==='undefined')return;const ro=new ResizeObserver(()=>{if(auto.current&&!gesture.current)fit();});ro.observe(el);return()=>ro.disconnect();},[source.width,source.height]);
  useEffect(()=>{
    fit();
    // Invalidate canvas surfaces after the split layout settles without changing authoring pixels.
    const frame=requestAnimationFrame(()=>{
      redraw(false);
      try{world.current?.querySelectorAll<HTMLCanvasElement>('.sie-paint canvas').forEach(canvas=>{
        const ctx=canvas.getContext('2d');if(!ctx)throw Error('canvas');
        ctx.save();try{ctx.globalCompositeOperation='copy';ctx.drawImage(canvas,0,0);}finally{ctx.restore();}
      });}catch{fail();}
    });
    return()=>cancelAnimationFrame(frame);
  },[layout]);
  useEffect(()=>{
    const el=viewport.current;if(!el)return;
    const wheel=(e:WheelEvent)=>{e.preventDefault();if(gesture.current)return;
      // Ctrl+wheel resizes the brush while Brush/Erase is active (about 10 % per notch, at least 1 px).
      if(e.ctrlKey&&brushWheel.current){const d=brushWheel.current.diameter,next=Math.round(d*Math.exp(-e.deltaY*.001));brushWheel.current.set(Math.max(1,Math.min(1024,next===d?d-Math.sign(e.deltaY):next)));return;}
      auto.current=false;const r=el.getBoundingClientRect();setView(v=>zoomAt(v,{x:e.clientX-r.left,y:e.clientY-r.top},v.scale*Math.exp(-e.deltaY*.001)));};
    el.addEventListener('wheel',wheel,{passive:false});return()=>el.removeEventListener('wheel',wheel);
  },[]);
  const priorMaskMode=useRef(maskMode);
  useEffect(()=>{
    if(maskMode&&!priorMaskMode.current&&activeMask?.visible)setTool('brush');
    if(!maskMode)setTool('select');
    priorMaskMode.current=maskMode;
  },[maskMode]);
  useEffect(()=>{report(masks.some(m=>m.visible&&m.content));redraw();},[masks]);
  useEffect(()=>{report(masksRef.current.some(m=>m.visible&&m.content));},[outpaint.length>0]);
  const previousImageIds=useRef(new Set(layers.map(l=>l.id)));
  useEffect(()=>{const added=layers.find(l=>!previousImageIds.current.has(l.id));previousImageIds.current=new Set(layers.map(l=>l.id));if(added){setActive(added.id);setTool('select');}},[layers]);
  useEffect(()=>()=>{gesture.current=null;},[]);
  function maskCanvas(){
    finish();const c=document.createElement('canvas');c.width=source.width;c.height=source.height;
    const ctx=c.getContext('2d');if(!ctx)throw new Error(label('canvasError'));
    if(!masksRef.current.some(m=>m.visible&&m.content)&&!outpaint.length)throw new Error(label('emptyMask'));
    ctx.fillStyle='#000';ctx.fillRect(0,0,c.width,c.height);
    ctx.fillStyle='#fff';for(const r of outpaint)ctx.fillRect(r.x,r.y,r.width,r.height);
    for(const m of masksRef.current)if(m.visible)ctx.drawImage(m.canvas,0,0);
    return c;
  }
  async function pngBytes(c:HTMLCanvasElement){
    const blob=await new Promise<Blob|null>(done=>c.toBlob(done,'image/png'));
    if(!blob)throw new Error(label('canvasError'));
    return new Uint8Array(await blob.arrayBuffer());
  }
  async function addMask(png:Uint8Array,name?:string){
    const version=operation.current;
    let bitmap:ImageBitmap|undefined;
    try{
      if(![137,80,78,71,13,10,26,10].every((v,i)=>png[i]===v))throw new Error('Invalid mask PNG');
      bitmap=await createImageBitmap(new Blob([new Uint8Array(png)],{type:'image/png'}));
      if(operation.current!==version)return false;
      if(bitmap.width!==source.width||bitmap.height!==source.height)throw new Error('Mask dimensions must match source');
      const mask=newMask(nextMask.current,source.width,source.height),ctx=mask.canvas.getContext('2d');
      if(!ctx)throw new Error(label('canvasError'));
      ctx.drawImage(bitmap,0,0);
      const image=ctx.getImageData(0,0,source.width,source.height),data=image.data;
      // Imported black is absent coverage, not an opaque eraser over earlier masks.
      for(let i=0;i<data.length;i+=4){const alpha=Math.round((data[i]+data[i+1]+data[i+2])/3*data[i+3]/255);data[i]=data[i+1]=data[i+2]=255;data[i+3]=alpha;if(alpha)mask.content=true;}
      if(!mask.content)return false;
      ctx.putImageData(image,0,0);mask.name=name;nextMask.current++;
      masksRef.current=[...masksRef.current,mask];setMasks(masksRef.current);contentRevision.current++;
      report(masksRef.current.some(m=>m.visible&&m.content));
      return true;
    }catch(error){if(operation.current!==version)return false;throw error;}
    finally{bitmap?.close();}
  }
  function refineMasks(){
    finish();
    const snapshots=masksRef.current.filter(m=>m.visible).flatMap(m=>{
      const c=document.createElement('canvas');c.width=source.width;c.height=source.height;
      const ctx=c.getContext('2d');if(!ctx)throw new Error(label('canvasError'));
      ctx.fillStyle='#000';ctx.fillRect(0,0,c.width,c.height);ctx.drawImage(m.canvas,0,0);
      const bounds=maskBounds(ctx.getImageData(0,0,c.width,c.height).data,c.width,c.height,0);
      return bounds?[{name:m.name??t('studio.editor.mask',{n:String(m.n)}),canvas:c,bounds}]:[];
    });
    return Promise.all(snapshots.map(async({name,canvas,bounds})=>({name,png:await pngBytes(canvas),bounds})));
  }
  useImperativeHandle(ref,()=>({addMask,refineMasks,sourcePng:()=>{finish();return flattenImages(images,source.width,source.height,outpaint.length?'#808080':undefined);},contentRevision:()=>contentRevision.current,capture:()=>{finish();return {source:{...source},masks:masksRef.current.map(m=>({...m,canvas:copyCanvas(m.canvas)})),base:copyImageLayer(base),baseIndex,nextMask:nextMask.current,active};},imageEdited,imagePng:()=>{finish();// Outpaint area starts neutral grey (like ImagePadForOutpaint) so denoise below 1 does not pull it to black.
    return imageEdited()?flattenImages(images,source.width,source.height,outpaint.length?'#808080':undefined):Promise.resolve(null);},maskBounds:(padding)=>{
    const c=maskCanvas(),bounds=maskBounds(c.getContext('2d')!.getImageData(0,0,c.width,c.height).data,c.width,c.height,padding);
    if(!bounds)throw new Error(label('emptyMask'));
    return bounds;
  },maskPng:async()=>pngBytes(maskCanvas())}));
  function point(e:{clientX:number;clientY:number}){return sourcePoint(e.clientX,e.clientY,world.current!.getBoundingClientRect(),source.width,source.height);}
  const samDisabled=!sam||sam.busy||!!sam.unavailable;
  function selectionBox(a:Point,b:Point){
    const x=Math.max(0,Math.min(source.width,Math.floor(Math.min(a.x,b.x)))),y=Math.max(0,Math.min(source.height,Math.floor(Math.min(a.y,b.y))));
    const right=Math.max(0,Math.min(source.width,Math.ceil(Math.max(a.x,b.x)))),bottom=Math.max(0,Math.min(source.height,Math.ceil(Math.max(a.y,b.y))));
    return {x,y,width:right-x,height:bottom-y};
  }
  function end(e:React.PointerEvent){
    const g=gesture.current;if(g?.pointer!==e.pointerId)return;
    if(g.kind==='samBox'&&!samDisabled&&maskMode){const box=selectionBox(g.start,point(e));finish();if(box.width>0&&box.height>0)sam!.onPick({box});return;}
    finish();
  }
  function begin(e:React.PointerEvent,layer?:ImageLayer,resize=false){
    if(gesture.current||(![0,1].includes(e.button)))return;
    if((tool==='samPoint'||tool==='samBox')&&e.button===0&&!layer){
      if(samDisabled||!maskMode)return;
      const p=point(e);if(!Number.isFinite(p.x)||!Number.isFinite(p.y))return;
      e.preventDefault();
      if(tool==='samPoint'){
        if(p.x<0||p.y<0||p.x>=source.width||p.y>=source.height)return;
        const key=e.altKey?'negativePoints':'positivePoints';
        samPoints.current={...samPoints.current,[key]:[...samPoints.current[key],{x:Math.floor(p.x),y:Math.floor(p.y)}]};
        sam!.onPick({...samPoints.current});
      }else{viewport.current?.setPointerCapture(e.pointerId);gesture.current={pointer:e.pointerId,kind:'samBox',start:p};setSamRect(selectionBox(p,p));}
      return;
    }
    if(tool==='wand'&&e.button===0&&!layer){e.preventDefault();void wand(point(e),e.altKey);return;}
    const pan=e.button===1||tool==='pan';
    if(!pan&&!layer&&(!brushEnabled||(tool!=='brush'&&tool!=='erase')))return;
    if(!pan&&layer&&tool!=='select')return;
    e.preventDefault();e.stopPropagation();viewport.current?.setPointerCapture(e.pointerId);
    if(pan){auto.current=false;gesture.current={pointer:e.pointerId,kind:'pan',start:{x:e.clientX,y:e.clientY},view};}
    else if(layer){setActive(layer.id);gesture.current={pointer:e.pointerId,kind:resize?'resize':'move',start:point(e),layer:{...layer}};}
    else if(activeImage){
      const l=activeImage,p=point(e);
      // Strokes live at the picture's own pixel size so moving or resizing the layer never resamples them.
      const c=l.paint??(()=>{const img=world.current?.querySelector<HTMLImageElement>(`[data-layer="${l.id}"] img`);const c=document.createElement('canvas');c.width=img?.naturalWidth||l.width;c.height=img?.naturalHeight||l.height;return c;})();
      const ctx=c.getContext('2d');if(!ctx){fail();return;}
      if(!l.paint)updateImages(images.map(v=>v.id===l.id?{...v,paint:c}:v));
      const k=c.width/l.width,map=(q:Point)=>({x:(q.x-l.x)*k,y:(q.y-l.y)*c.height/l.height});
      gesture.current={pointer:e.pointerId,kind:'paint',ctx,map,k,last:p,erase:tool==='erase',diameter,shape,color};
      stampStroke(ctx,map(p),map(p),diameter*k,shape,tool==='erase',color);contentRevision.current++;
    }
    else if(activeMask){
      const p=point(e);if(p.x<0||p.y<0||p.x>=source.width||p.y>=source.height)return;
      const ctx=activeMask.canvas.getContext('2d');if(!ctx){fail();return;}
      const value=`rgb(${gray},${gray},${gray})`;
      gesture.current={pointer:e.pointerId,kind:'paint',ctx,mask:activeMask,map:p=>p,k:1,last:p,erase:tool==='erase',diameter,shape,color:value};
      stampStroke(ctx,p,p,diameter,shape,tool==='erase',value);redraw();
    }
  }
  function move(e:React.PointerEvent){
    track(e);
    const g=gesture.current;if(!g||g.pointer!==e.pointerId)return;
    if(g.kind==='pan'){setView({...g.view,x:g.view.x+e.clientX-g.start.x,y:g.view.y+e.clientY-g.start.y});return;}
    const p=point(e);
    if(g.kind==='samBox'){setSamRect(selectionBox(g.start,p));return;}
    if(g.kind==='paint'){stampStroke(g.ctx,g.map(g.last),g.map(p),g.diameter*g.k,g.shape,g.erase,g.color);contentRevision.current++;g.last=p;if(g.mask)redraw();return;}
    const dx=Math.round(p.x-g.start.x),dy=Math.round(p.y-g.start.y),l=g.layer;
    const width=Math.max(8,Math.min(8192,l.width+dx));
    updateImages(images.map(v=>v.id!==l.id?v:g.kind==='move'?{...v,x:Math.max(1-l.width,Math.min(source.width-1,l.x+dx)),y:Math.max(1-l.height,Math.min(source.height-1,l.y+dy))}:{...v,width,height:Math.max(8,Math.min(8192,Math.round(width*l.height/l.width)))}));
  }
  function imageKey(e:React.KeyboardEvent,l:ImageLayer){
    if(tool!=='select')return;const step=e.shiftKey?64:8;
    const dx=e.key==='ArrowRight'?step:e.key==='ArrowLeft'?-step:0,dy=e.key==='ArrowDown'?step:e.key==='ArrowUp'?-step:0;
    if(e.key==='Delete'||e.key==='Backspace'){e.preventDefault();updateImages(images.filter(v=>v.id!==l.id));setActive('base');return;}
    const grow=e.key==='+'?step:e.key==='-'?-step:0;if(!dx&&!dy&&!grow)return;e.preventDefault();
    const width=Math.max(8,Math.min(8192,l.width+grow));
    updateImages(images.map(v=>v.id!==l.id?v:{...v,x:Math.max(1-l.width,Math.min(source.width-1,l.x+dx)),y:Math.max(1-l.height,Math.min(source.height-1,l.y+dy)),...(grow?{width,height:Math.max(8,Math.min(8192,Math.round(width*l.height/l.width)))}:{})}));
  }
  function restoreFrame(){
    finish();if(!activeImage)return;
    const l=activeImage,k=Math.min(1,source.width/l.width,source.height/l.height);
    const width=l.id==='base'?source.width:Math.max(1,Math.round(l.width*k)),height=l.id==='base'?source.height:Math.max(1,Math.round(l.height*k));
    const x=l.id==='base'?0:Math.max(0,Math.min(source.width-width,l.x)),y=l.id==='base'?0:Math.max(0,Math.min(source.height-height,l.y));
    updateImages(images.map(v=>v.id===l.id?{...v,x,y,width,height}:v));fit();
  }
  function reorder(delta:number){
    const swap=<T,>(list:T[],i:number)=>{const j=i+delta;if(i<0||j<0||j>=list.length)return null;const next=[...list];[next[i],next[j]]=[next[j],next[i]];return next;};
    if(activeMask){const next=swap(masks,masks.indexOf(activeMask));if(next)setMasks(next);return;}
    const next=swap(images,images.findIndex(l=>l.id===active));if(next)updateImages(next);
  }
  const stack=activeMask?masks:images,stackId=(x:Mask|ImageLayer)=>'n' in x?`mask:${x.n}`:x.id;
  function dropLayer(e:React.DragEvent,target:string){
    const id=draggedLayer.current;if(!id||id===target)return;
    const fromMask=masks.findIndex(m=>`mask:${m.n}`===id),toMask=masks.findIndex(m=>`mask:${m.n}`===target);
    const fromImage=images.findIndex(l=>l.id===id),toImage=images.findIndex(l=>l.id===target);
    if((fromMask<0||toMask<0)&&(fromImage<0||toImage<0))return;
    e.preventDefault();e.stopPropagation();finish();draggedLayer.current=null;
    if(fromMask>=0&&toMask>=0){const next=[...masks];next.splice(toMask,0,next.splice(fromMask,1)[0]);setMasks(next);}
    else{const next=[...images];next.splice(toImage,0,next.splice(fromImage,1)[0]);updateImages(next);}
  }
  function row(id:string,name:string,thumb:React.ReactNode,visible:boolean,choose:()=>void,toggle?:()=>void,remove?:()=>void){return <li key={id} draggable onDragStart={e=>{finish();draggedLayer.current=id;e.dataTransfer.setData(LAYER_DRAG,id);e.dataTransfer.effectAllowed='move';}} onDragEnd={()=>{draggedLayer.current=null;}} onDragOver={e=>{if(draggedLayer.current&&e.dataTransfer.types.includes(LAYER_DRAG)){e.preventDefault();e.dataTransfer.dropEffect='move';}}} onDrop={e=>dropLayer(e,id)}><button type="button" aria-pressed={active===id} onClick={()=>{finish();setActive(id);choose();}}>{thumb}<span>{name}</span></button>{toggle&&<button type="button" aria-label={t(`studio.editor.${visible?'hide':'show'}`,{name})} className="sie-eye" onClick={()=>{finish();toggle();}}><Icon d={visible?icons.eye:icons.eyeOff}/></button>}{remove&&<button type="button" aria-label={t('studio.editor.remove',{name})} onClick={()=>{finish();remove();}}>×</button>}</li>;}
  return <section className="sie" aria-label={label('title')}>
    <div className="sie-tools" role="toolbar" aria-label={label('title')}>
      <span className="sie-seg">{(maskMode?['select','brush','erase','wand','samPoint','samBox','pan'] as const:['select','brush','erase','pan'] as const).map(v=><button type="button" className="sie-tool" key={v} aria-pressed={tool===v} data-sam-tool={v==='samPoint'||v==='samBox'?v:undefined} title={v==='samPoint'||v==='samBox'?sam?.unavailable??(sam?.busy?t('setup.busy'):label(`${v}Help`)):undefined} disabled={((v==='samPoint'||v==='samBox')&&samDisabled)||((v==='brush'||v==='erase')&&!brushEnabled)||(v==='wand'&&!activeMask?.visible)} onClick={()=>{finish();setTool(v);}}><Icon d={icons[v]}/>{label(v)}</button>)}</span>
      <>
      {tool==='wand'&&maskMode&&<label title={label('wandHelp')}>{label('tolerance')}<input type="range" aria-label={label('tolerance')} min={0} max={255} value={tolerance} onChange={e=>setTolerance(Number(e.target.value))}/><output>{tolerance}</output></label>}
      <label>{label('diameter')}<input type="number" min={1} max={1024} value={diameter} onChange={e=>{const n=Number(e.target.value);if(diameterOK(n))setDiameter(n);}}/></label>
      <input type="range" className="sie-diameter" aria-label={label('diameter')} min={1} max={1024} value={diameter} onChange={e=>setDiameter(Number(e.target.value))}/>
      <span className="sie-seg" role="group" aria-label={label('shape')}>{(['round','square'] as const).map(v=><button type="button" className="sie-tool" key={v} aria-pressed={shape===v} onClick={()=>setShape(v)}><i className={`sie-shape is-${v}`} aria-hidden="true"/>{label(v)}</button>)}</span>
      {activeImage&&<span className="sie-color" role="group" aria-label={label('color')}>
       <input type="color" aria-label={label('color')} value={color} onChange={e=>setColor(e.target.value)}/>
       <input type="text" className="sie-hex" aria-label={label('colorHex')} value={hex} size={7} maxLength={7} spellCheck={false} onChange={e=>{setHex(e.target.value);if(hexOK(e.target.value))setColor(e.target.value.toLowerCase());}}/>
       {palette.map(c=><button key={c} type="button" className="sie-swatch" aria-label={c} aria-pressed={color===c} style={{background:c}} onClick={()=>setColor(c)}/>)}
      </span>}
      {maskMode&&activeMask&&<label>{label('maskValue')}<input type="range" className="sie-gray" min={0} max={255} value={gray} style={{'--gray':`rgb(${gray},${gray},${gray})`} as React.CSSProperties} onChange={e=>setGray(Number(e.target.value))}/></label>}
      <button type="button" className="sie-tool" disabled={!brushEnabled} onClick={()=>{finish();const m=activeMask,l=activeImage;try{if(l?.paint){const ctx=l.paint.getContext('2d');if(!ctx)throw Error();ctx.clearRect(0,0,l.paint.width,l.paint.height);contentRevision.current++;return;}if(!m)return;const ctx=m.canvas.getContext('2d');if(!ctx)throw Error();ctx.clearRect(0,0,source.width,source.height);m.content=false;report(masks.some(v=>v.visible&&v.content));redraw();}catch{fail();}}}>{t('studio.run.edit.clear')}</button>
      </>
      <span className="sie-view">
      <button type="button" data-image-restore disabled={!activeImage} onClick={restoreFrame}>{label('restoreFrame')}</button>
      <button type="button" aria-label={label('zoomOut')} onClick={()=>{finish();auto.current=false;setView(v=>zoomAt(v,{x:(viewport.current?.clientWidth??0)/2,y:(viewport.current?.clientHeight??0)/2},v.scale/1.25));}}>−</button>
      <button type="button" onClick={()=>{finish();fit();}}>{label('fit')}</button>
      <button type="button" onClick={()=>{finish();auto.current=false;setView(v=>zoomAt(v,{x:(viewport.current?.clientWidth??0)/2,y:(viewport.current?.clientHeight??0)/2},1));}}>{label('actual')}</button>
      <button type="button" aria-label={label('zoomIn')} onClick={()=>{finish();auto.current=false;setView(v=>zoomAt(v,{x:(viewport.current?.clientWidth??0)/2,y:(viewport.current?.clientHeight??0)/2},v.scale*1.25));}}>+</button>
      {maskMode&&<label>{label('overlay')}<input type="range" aria-label={label('overlay')} list="sie-overlay-stops" min={0} max={100} value={opacity} onChange={e=>setOpacity(snapOverlay(Number(e.target.value)))}/><datalist id="sie-overlay-stops">{overlayStops.map(v=><option key={v} value={v}/>)}</datalist><output>{opacity}%</output></label>}
      </span>
    </div>
    <div className="sie-body">
      <div ref={viewport} className="sie-viewport" data-editor-viewport data-tool={painting&&!brushEnabled?'off':tool} onPointerDown={e=>{track(e);begin(e);}} onPointerMove={move} onPointerLeave={()=>{if(cursor.current)cursor.current.hidden=true;}} onPointerUp={end} onPointerCancel={finish} onLostPointerCapture={finish} onDragOver={e=>{if(e.dataTransfer.types.includes(MEDIA_DRAG))e.preventDefault();}} onDrop={e=>{const item=draggedItem(e.dataTransfer);if(item){e.preventDefault();finish();onInsert(item,point(e));}}}>
        <div ref={world} className="sie-world" data-testid="editor-world" style={{width:source.width,height:source.height,transform:`translate(${view.x}px, ${view.y}px) scale(${view.scale})`}}>
          {images.filter(l=>l.visible).map(l=><div className={`sie-image${active===l.id?' is-selected':''}`} key={l.id} role="group" aria-label={l.id==='base'?label('base'):t('studio.run.edit.layer',{name:l.name})} tabIndex={0} data-layer={l.id} style={{left:l.x,top:l.y,width:l.width,height:l.height,pointerEvents:tool==='select'?'auto':'none'}} onFocus={()=>setActive(l.id)} onPointerDown={e=>begin(e,l)} onKeyDown={e=>imageKey(e,l)}><img src={l.src} alt={l.id==='base'?source.name:''} draggable={false}/>{l.paint&&<Mounted node={l.paint}/>}{active===l.id&&tool==='select'&&<button className="sie-resize" type="button" aria-label={t('studio.editor.resize',{name:l.name})} onPointerDown={e=>begin(e,l,true)}>↘</button>}</div>)}
          <canvas ref={overlay} className="sie-mask" aria-label={t('studio.run.edit.mask')} width={source.width} height={source.height} style={{opacity:maskMode?opacity/100:0}}/>
          {samRect&&<div className="sie-sam-box" aria-hidden="true" style={{left:samRect.x,top:samRect.y,width:samRect.width,height:samRect.height,borderWidth:1/view.scale}}/>}
          <div className="sie-frame" data-source-frame aria-hidden="true" style={{outlineWidth:2/view.scale,outlineOffset:-2/view.scale}}/>
        </div>
        {painting&&brushEnabled&&<div ref={cursor} className={`sie-cursor is-${shape}${tool==='erase'?' is-erase':''}`} hidden aria-hidden="true" style={{width:diameter*view.scale,height:diameter*view.scale}}/>}
      </div>
      <aside className="sie-rail" aria-label={label('layers')}>
        {maskMode&&<button type="button" disabled={masks.length>=4} onClick={()=>{finish();const n=nextMask.current++;setMasks(v=>[...v,newMask(n,source.width,source.height)]);setActive(`mask:${n}`);setTool('brush');}}>{label('addMask')}</button>}
        <ul>{maskMode&&[...masks].reverse().map(m=>{const name=m.name??t('studio.editor.mask',{n:String(m.n)});return row(`mask:${m.n}`,name,<Thumbnail mask={m} revision={revision}/>,m.visible,()=>setTool('brush'),()=>setMasks(v=>v.map(x=>x.id===m.id?{...x,visible:!x.visible}:x)),()=>{setMasks(v=>v.filter(x=>x.id!==m.id));if(active===`mask:${m.n}`){setActive('base');setTool('select');}});})}
          {[...images].reverse().map(l=>row(l.id,l.id==='base'?label('base'):l.name,<img src={l.src} alt=""/>,l.visible,()=>setTool('select'),()=>updateImages(images.map(x=>x.id===l.id?{...x,visible:!x.visible}:x)),l.id==='base'?undefined:()=>{updateImages(images.filter(x=>x.id!==l.id));if(active===l.id)setActive('base');}))}</ul>
        <button type="button" disabled={(!activeImage&&!activeMask)||stackId(stack.at(-1)!)===active} onClick={()=>reorder(1)}>{label('raise')}</button>
        <button type="button" disabled={(!activeImage&&!activeMask)||stackId(stack[0])===active} onClick={()=>reorder(-1)}>{label('lower')}</button>
      </aside>
    </div>{error&&<p role="alert">{error}</p>}
  </section>;
}
