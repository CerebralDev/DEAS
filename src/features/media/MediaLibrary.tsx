import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useI18n } from '../../i18n/I18nProvider';
import { describeMedia, draggedItem, dragItem, dropCssPoint, MEDIA_DRAG, itemFromEntry, itemFromFile, listMedia, normalizedMediaPath as normalizedPath, pasteToInput, pastedPictures, type MediaEntry, type MediaItem, type MediaListing } from '../../lib/media';
import { usePref } from '../../lib/preferences';
import { StarMediaButton } from './StarMediaButton';
import { relocatedMediaItem, type MediaRelocation } from '../../lib/media';
import './media.css';
const depths=[1,2,3,4,5];
const isDepth=(v:unknown):v is number=>typeof v==='number'&&depths.includes(v);

const treeAnchor=(path:string)=>/^(?:\/|[A-Za-z]:[\\/])$/.test(path)?path:path.replace(/[\\/]+$/,'');
const underRoot=(path:string,root?:string)=>!!root&&(normalizedPath(path)===normalizedPath(root)||normalizedPath(path).startsWith(normalizedPath(root)+'/'));
// Session caches: same-folder refreshes paint instantly; thumbnails skip the IPC round trip.
const listings=new Map<string,MediaListing>();
const thumbs=new Map<string,string>();
/** First frame only: an animated <img> tile keeps WebKitGTK's decoder threads busy and the window repainting for as long as it is shown. */
function Still({src}:{src:string}){
 const ref=useRef<HTMLCanvasElement>(null);
 useEffect(()=>{const img=new Image();img.onload=()=>{const c=ref.current,ctx=c?.getContext('2d');if(c&&ctx){c.width=img.naturalWidth;c.height=img.naturalHeight;ctx.drawImage(img,0,0);}};img.src=src;},[src]);
 return <canvas ref={ref} data-still aria-hidden="true"/>;
}
/** Image tile: asks the native side for its cached small PNG, falls back to the original file (its first frame for formats that may animate).
 *  Video tile: the first frame as a cached FFmpeg still (no <video> per tile); the ▶ glyph stays when extraction fails. Tiles only mount while visible. */
function Thumb({entry}:{entry:MediaEntry}){
 const video=entry.media==='video';
 const key=`${entry.path}\0${entry.size}\0${entry.modified}`;
 const [src,setSrc]=useState(()=>thumbs.get(key)??'');
 useEffect(()=>{
  if(thumbs.has(key)){setSrc(thumbs.get(key)!);return;}
  let alive=true;
  const still=video?invoke<string>('native_media_reference_thumbnail',{path:entry.path,seconds:0}).then(p=>typeof p==='string'?convertFileSrc(p):'',()=>'')
   :invoke<string>('native_media_thumbnail',{path:entry.path}).then(p=>typeof p==='string'?p:entry.path,()=>entry.path).then(convertFileSrc);
  void still.then(s=>{thumbs.set(key,s);if(alive)setSrc(s);});
  return()=>{alive=false;};
 },[key]); // eslint-disable-line react-hooks/exhaustive-deps
 if(video)return src?<span className="media-video-still"><img src={src} alt="" loading="lazy" decoding="async"/><span className="media-video-badge" aria-hidden="true">▶</span></span>:<span className="media-glyph" aria-hidden="true">▶</span>;
 if(!src)return <span className="media-glyph" aria-hidden="true"/>;
 return src===convertFileSrc(entry.path)&&/\.(webp|gif|apng)$/i.test(entry.name)?<Still src={src}/>:<img src={src} alt="" loading="lazy" decoding="async"/>;
}
/** First-frame still of a local video by path (queue rows); `fallback` while loading or when FFmpeg cannot read it. */
export function VideoStill({path,fallback}:{path:string;fallback:React.ReactNode}){
 const [src,setSrc]=useState(()=>thumbs.get(path)??'');
 useEffect(()=>{
  if(thumbs.has(path)){setSrc(thumbs.get(path)!);return;}
  let alive=true;
  void invoke<string>('native_media_reference_thumbnail',{path,seconds:0}).then(p=>typeof p==='string'?convertFileSrc(p):'',()=>'').then(s=>{thumbs.set(path,s);if(alive)setSrc(s);});
  return()=>{alive=false;};
 },[path]);
 return src?<img src={src} alt="" decoding="async"/>:<>{fallback}</>;
}
/** Renders only the tile rows in view plus two spare rows; unmeasured (jsdom, empty) renders everything. `folder` changes scroll back to the top. */
function Tiles<T>({items,render,folder,layout,...rest}:{items:T[];render:(item:T)=>React.ReactNode;folder:string;layout:MediaView}&React.HTMLAttributes<HTMLUListElement>){
 // The scroll box owns the height (flex); the padding spacers sit on the inner ul. Padding on a flex item cannot shrink, so the item grows to it and the ResizeObserver loops (flicker).
 const box=useRef<HTMLDivElement>(null);
 const [view,setView]=useState({top:0,height:0,cols:0,rowH:1});
 const measure=()=>{const el=box.current,li=el?.firstElementChild?.firstElementChild as HTMLElement|null;if(!el||!li?.offsetWidth)return;const gap=6;const next={top:el.scrollTop,height:el.clientHeight,cols:Math.max(1,Math.floor((el.clientWidth+gap)/(li.offsetWidth+gap))),rowH:li.offsetHeight+gap};setView(v=>v.top===next.top&&v.height===next.height&&v.cols===next.cols&&v.rowH===next.rowH?v:next);};
 useLayoutEffect(()=>{if(box.current)box.current.scrollTop=0;},[folder]);
 useLayoutEffect(()=>{measure();if(typeof ResizeObserver==='undefined'||!box.current)return;const ro=new ResizeObserver(measure);ro.observe(box.current);return()=>ro.disconnect();},[items.length,layout]); // another view changes tile size without resizing the box
 const rows=view.cols?Math.ceil(items.length/view.cols):0;
 const first=view.cols?Math.min(Math.max(0,rows-1),Math.max(0,Math.floor(view.top/view.rowH)-2)):0;
 const last=view.cols?Math.min(rows,Math.ceil((view.top+view.height)/view.rowH)+2):rows;
 return <div ref={box} className="media-scroll" onScroll={measure}><ul style={view.cols?{paddingTop:2+first*view.rowH,paddingBottom:4+(rows-last)*view.rowH}:undefined} {...rest}>{(view.cols?items.slice(first*view.cols,last*view.cols):items).map(render)}</ul></div>;
}
/** Environment folders the backend reads from and writes to; shown as quick chips. */
export interface MediaRoots { input?: string; output?: string }
export type MediaSort='date'|'name'|'size';
export const mediaViews=['tiles','thumbs','list'] as const;
export type MediaView=typeof mediaViews[number];
export const mediaTypes=['all','image','video','audio'] as const;
export type MediaType=typeof mediaTypes[number];
/** Extra right-click entry; `blocked` (a reason) shows it disabled with that reason. */
/** A right-click entry for a media file; Studio mirrors every entry as a button in the preview toolbar. */
export interface MediaAction { label:string; onClick:()=>void; blocked?:string; /** Tooltip while enabled. */ hint?:string; /** Picks the file as the run's input, so the preview omits its generic "Use as input". */ input?:boolean }
export interface MediaLibraryProps { selected: MediaItem|null; onSelect: (item: MediaItem) => void; roots?: MediaRoots; /** Bump after a generation finished so an open output folder re-lists. */ outputRevision?: number; /** Case-insensitive name filter. */ filter?: string; /** Files by newest, name or largest; folders live in the navigator. */ sort?: MediaSort; /** Tiles, large thumbnails or a name list. */ view?: MediaView; /** Only this media type (folders stay in the navigator). */ type?: MediaType; /** Badge per file path, e.g. a reference label. */ marks?: Map<string,string>; /** Feature entries for a file's right-click menu. */ actions?: (item:MediaItem)=>MediaAction[] }
/** Media browser with explicit native-authorized physical starring. */
export function MediaLibrary({selected,onSelect,roots={},outputRevision=0,filter='',sort='date',view='tiles',type='all',marks,actions}:MediaLibraryProps){
 const {t}=useI18n();
 const dropBox=useRef<HTMLElement>(null),selectRef=useRef(onSelect);selectRef.current=onSelect;
 const native=isTauri();
 const [listing,setListing]=useState<MediaListing|null>(null);
 const [treeRoot,setTreeRoot]=useState('');
 const [directories,setDirectories]=useState<Record<string,MediaListing>>({});
 const [expanded,setExpanded]=useState<Set<string>>(()=>new Set());
 const [folderErrors,setFolderErrors]=useState<Record<string,string>>({});
 const pendingFolders=useRef(new Set<string>());
 const mounted=useRef(false);
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
 const [dropped,setDropped]=useState<MediaItem[]>([]);
 const [error,setError]=useState('');
 const [dragging,setDragging]=useState(false);
 // Explicit copy-in field: a pasted picture or a dropped picture/video becomes a new file in Input.
 const intake=useRef<HTMLInputElement>(null);const [intakeStatus,setIntakeStatus]=useState('');
 // Right-click menu on a file tile: open its folder in the system file manager (native only) plus the host's feature entries.
 const [menu,setMenu]=useState<{x:number;y:number;item:MediaItem;actions:MediaAction[]}|null>(null);
 useEffect(()=>{if(!menu)return;const close=()=>setMenu(null);const key=(e:KeyboardEvent)=>{if(e.key==='Escape')close();};
  // Wheel, not scroll: right-clicking a partly hidden tile focuses it and its box scrolls it into view, which must not close the menu just opened.
  window.addEventListener('click',close);window.addEventListener('keydown',key);window.addEventListener('resize',close);window.addEventListener('wheel',close,true);
  return()=>{window.removeEventListener('click',close);window.removeEventListener('keydown',key);window.removeEventListener('resize',close);window.removeEventListener('wheel',close,true);};},[menu]);
 const onMenu=(item:MediaItem|null)=>(e:React.MouseEvent)=>{const extra=item?actions?.(item)??[]:[];if(!item||(!(native&&item.path)&&!extra.length))return;e.preventDefault();setMenu({x:e.clientX,y:e.clientY,item,actions:extra});};
 const reveal=(path:string)=>{setMenu(null);invoke('native_media_reveal',{path}).catch(e=>setError(String(e)));};
 const version=useRef(0);
 const [depth,setDepth]=usePref('media.depth',3,isDepth);
 const rawRoot=roots.output?roots.output.replace(/[\\/]+$/,'')+'/raw':undefined;
 const starredRoot=roots.output?roots.output.replace(/[\\/]+$/,'')+'/starred':undefined;
 const open=async(path:string,levels=depth,picked=false)=>{
  const v=++version.current;
  // Cached refreshes may paint immediately, but navigation commits only after a successful listing.
  const key=`${path}\0${levels}`;const cached=listing?.path===path?listings.get(key):undefined;if(cached){setListing(cached);setError('');}
  try{const next=await listMedia(path,levels);listings.set(key,next);if(v===version.current){setListing(next);setError('');setDirectories(d=>({...d,[next.path]:next}));setFolderErrors(errors=>({...errors,[next.path]:''}));setTreeRoot(root=>underRoot(next.path,roots.output)?treeAnchor(roots.output!):underRoot(next.path,roots.input)?treeAnchor(roots.input!):!picked&&underRoot(next.path,root)?root:next.path);setExpanded(paths=>new Set([...paths,next.path]));}}
  catch(e){listings.delete(key);if(v===version.current){setError(String(e));if(cached)setListing(l=>l===cached?null:l);}} // a stale cached view must not outlive a folder that failed to list
 };
 // The output folder is the natural start once an environment is known; before that, home.
 useEffect(()=>{if(native)void open(rawRoot??roots.input??'');return()=>{version.current++;};},[native,roots.output,roots.input]);
 const inOutput=!!listing&&underRoot(listing.path,roots.output);
 const inStarred=!!listing&&underRoot(listing.path,starredRoot);
 const canStar=(item:MediaItem)=>native&&!!item.path&&underRoot(item.path,roots.output)&&!underRoot(item.path,roots.input);
 useEffect(()=>{if(outputRevision&&inOutput&&listing)void open(listing.path);},[outputRevision]); // eslint-disable-line react-hooks/exhaustive-deps
 useEffect(()=>{
  const moved=(event:Event)=>{const move=(event as CustomEvent<MediaRelocation>).detail;listings.clear();thumbs.clear();setDirectories({});setDropped(items=>items.map(item=>relocatedMediaItem(item,move)));if(listing)void open(listing.path);};
  window.addEventListener('deas-media-relocated',moved);return()=>window.removeEventListener('deas-media-relocated',moved);
 },[listing]);
 const changeDepth=(levels:number)=>{setDepth(levels);if(listing)void open(listing.path,levels);};
 const rootName=underRoot(treeRoot,roots.output)?'root':underRoot(treeRoot,roots.input)?t('media.root.input'):treeRoot.split(/[\\/]+/).filter(Boolean).at(-1)??treeRoot;
 const parent=listing?.parent&&underRoot(listing.parent,treeRoot)&&normalizedPath(listing.path)!==normalizedPath(treeRoot)?listing.parent:null;
 const addDropped=(items:(MediaItem|null)[])=>{const valid=items.filter((i):i is MediaItem=>!!i);if(valid.length){setDropped(d=>[...valid,...d]);selectRef.current(valid[0]);}};
 const dropPaths=async(paths:string[])=>{
  try{const entries=await describeMedia(paths);if(!mounted.current)return;const folder=entries.find(e=>e.kind==='directory');if(folder)void open(folder.path,depth,true);addDropped(entries.map(itemFromEntry));}
  catch(e){setError(String(e));}
 };
 const currentDrop=useRef(dropPaths);currentDrop.current=dropPaths;
 const ingested=async(paths:string[])=>{
  if(!mounted.current||!paths.length)return;
  if(roots.input)await open(roots.input,depth,true);
  const item=itemFromEntry((await describeMedia(paths.slice(-1)))[0]);
  if(item&&mounted.current){selectRef.current(item);setIntakeStatus(t('media.intake.done',{name:item.name}));}
 };
 const pasteFiles=async(files:Blob[])=>{
  try{await ingested(await pasteToInput(files));}
  catch(e){if(mounted.current)setIntakeStatus(String(e));}
 };
 const copyIn=async(paths:string[])=>{
  try{const copied:string[]=[];for(const path of paths)copied.push(await invoke<string>('native_media_copy_in',{path}));await ingested(copied);}
  catch(e){if(mounted.current)setIntakeStatus(String(e));}
 };
 const intakeActions=useRef({pasteFiles,copyIn});intakeActions.current={pasteFiles,copyIn};
 // Ctrl+V while the panel (or nothing) has focus; text fields keep their own paste and a modal owns its pastes.
 useEffect(()=>{
  if(!native||!roots.input)return;
  const paste=(event:ClipboardEvent)=>{
   const el=dropBox.current,target=event.target;
   if(event.defaultPrevented||!el||el.closest('[hidden]')||!(target instanceof Element))return;
   const modal=document.querySelector('dialog[open], [aria-modal="true"]');
   if((modal&&!modal.contains(el))||!(el.contains(target)||target===document.body)||(target!==intake.current&&target.closest('input, textarea, [contenteditable="true"]')))return;
   const media=target===intake.current?[...(event.clipboardData?.files??[])].filter(f=>f.type.startsWith('image/')):pastedPictures(event);
   if(!media.length){if(event.clipboardData?.files.length||target===intake.current)setIntakeStatus(t('media.intake.ignored'));return;}
   event.preventDefault();void intakeActions.current.pasteFiles(media);
  };
  document.addEventListener('paste',paste);return()=>document.removeEventListener('paste',paste);
 },[native,roots.input]);
 // Native webviews deliver OS drops as paths; the HTML5 handlers below serve the browser preview.
 useEffect(()=>{
  if(!native)return;
  let alive=true,unlisten:(()=>void)|undefined;
  void (async()=>{
   try{const stop=await getCurrentWebview().onDragDropEvent(e=>{
    // A modal owns its drops; retained background browsers must not consume them too.
    const modal=document.querySelector('dialog[open], [aria-modal="true"]'),el=dropBox.current;
    if(!el||el.closest('[hidden]')||(modal&&!modal.contains(el))){setDragging(false);return;}
    if(e.payload.type!=='leave'){
     const r=el.getBoundingClientRect(),{x,y}=dropCssPoint(e.payload.position);
     if(r.width<=0||r.height<=0||x<r.left||x>r.right||y<r.top||y>r.bottom){setDragging(false);return;}
     const field=intake.current?.getBoundingClientRect();
     if(e.payload.type==='drop'&&field&&field.width>0&&x>=field.left&&x<=field.right&&y>=field.top&&y<=field.bottom){setDragging(false);void intakeActions.current.copyIn(e.payload.paths);return;}
    }
    if(e.payload.type==='enter'||e.payload.type==='over')setDragging(true);
    else{setDragging(false);if(e.payload.type==='drop')void currentDrop.current(e.payload.paths);}
   });if(alive)unlisten=stop;else stop();}
   catch(e){if(alive)setError(String(e));} // drag & drop unavailable must not take the library down
  })();
  return()=>{alive=false;unlisten?.();};
 },[native]);
 const pick=async()=>{
  try{const folder=await invoke<string|null>('native_pick_directory',{title:t('media.pickTitle')});if(folder)void open(folder,depth,true);}
  catch(e){setError(String(e));}
 };
 const choose=(entry:MediaEntry)=>{const item=itemFromEntry(entry);if(item)onSelect(item);};
 const visible=useMemo(()=>{const q=filter.trim().toLowerCase();return listing?.entries.filter(e=>e.kind==='file'&&e.media&&(type==='all'||e.media===type)&&(!q||e.name.toLowerCase().includes(q)))
  .sort((a,b)=>sort==='date'?b.modified-a.modified:sort==='size'?b.size-a.size:a.name.localeCompare(b.name))??[];},[listing,filter,sort,type]);
 const loadFolder=async(path:string)=>{
  if(pendingFolders.current.has(path))return;
  pendingFolders.current.add(path);
  setFolderErrors(errors=>({...errors,[path]:''}));
  try{const next=await listMedia(path,1);if(mounted.current)setDirectories(d=>({...d,[path]:next}));}
  catch(e){if(mounted.current)setFolderErrors(errors=>({...errors,[path]:String(e)}));}
  finally{pendingFolders.current.delete(path);}
 };
 const toggleFolder=(path:string)=>{
  if(expanded.has(path)){setExpanded(paths=>{const next=new Set(paths);next.delete(path);return next;});return;}
  setExpanded(paths=>new Set([...paths,path]));
  if(!directories[path])void loadFolder(path);
 };
 useEffect(()=>{
  if(!listing||!treeRoot)return;
  const relative=normalizedPath(listing.path).slice(normalizedPath(treeRoot).length).split('/').filter(Boolean);
  const separator=listing.path.includes('\\')?'\\':'/';
  const parents=[treeRoot,...relative.slice(0,-1).map((_,i)=>normalizedPath(treeRoot).replace(/\//g,separator)+separator+relative.slice(0,i+1).join(separator))];
  setExpanded(paths=>new Set([...paths,...parents]));
  parents.forEach((path,i)=>{
   const child=i<relative.length?relative[i]:undefined;
   if(!directories[path]||(child&&!directories[path].entries.some(e=>e.kind==='directory'&&e.name===child)))void loadFolder(path);
  });
 },[treeRoot,listing]); // Reuse expansion loading, including fixed children just created by native listing.
 const folderBranch=(path:string,name:string,symlink=false,ancestors:string[]=[]):React.ReactNode=>{
  const data=directories[path],isOpen=expanded.has(path);
  const children=data?.entries.filter(e=>e.kind==='directory'&&e.path!==path&&!ancestors.includes(e.path)).sort((a,b)=>a.name.localeCompare(b.name));
  return <li key={path}><div className="media-folder-row"><button type="button" className="media-folder-toggle" aria-expanded={isOpen} aria-label={t(isOpen?'media.collapseFolder':'media.expandFolder',{name})} onClick={()=>void toggleFolder(path)}>{isOpen?'▾':'▸'}</button><button type="button" className="media-folder-name" aria-label={name} title={path} aria-current={listing?.path===path?'location':undefined} onClick={()=>void open(path)}>{name}{symlink&&<span className="media-symlink" aria-label={t('media.symlink')}> ⤳</span>}</button></div>
   {isOpen&&(folderErrors[path]?<p className="media-error" role="alert">{folderErrors[path]}</p>:data?<><ul>{children?.map(e=>folderBranch(e.path,e.name,e.symlink,[...ancestors,path]))}</ul>{data.truncated&&listing?.path!==path&&<p className="media-help">{t('media.truncated')}</p>}</>:<p className="media-help" role="status">{t('media.loadingFolders')}</p>)}
  </li>;
 };
 // Videos get FFmpeg stills, never <video> tiles: each would open one GStreamer pipeline in WebKitGTK.
 const pathMarks=useMemo(()=>new Map([...(marks??[])].map(([path,label])=>[normalizedPath(path),label])),[marks]);
 const mark=(path?:string)=>path?pathMarks.get(normalizedPath(path)):undefined;
 // Beside the button, so the tile's accessible name stays the file name.
 const badge=(path?:string)=>mark(path)&&<span className="media-mark">{mark(path)}</span>;
 const thumb=(media:string|null,src:string|null)=>media==='image'&&src?<img src={src} alt="" loading="lazy"/>:<span className="media-glyph" aria-hidden="true">{media==='video'?'▶':media==='audio'?'♪':'▸'}</span>;
 return <aside ref={dropBox} className={`media-library${dragging?' is-dragging':''}`} data-view={view} aria-label={t('media.title')}
  onDragOver={e=>{e.preventDefault();setDragging(true);}} onDragLeave={()=>setDragging(false)}
  onDrop={e=>{e.preventDefault();setDragging(false);addDropped([...e.dataTransfer.files].map(itemFromFile));}}>
  <header className="media-header"><h2>{t('media.title')}</h2>
   {(roots.input||roots.output)&&<div className="media-roots" role="group" aria-label={t('media.roots')}>{(['output','starred','input'] as const).map(key=>{const path=key==='output'?rawRoot:key==='starred'?starredRoot:roots.input;return path&&<button key={key} type="button" data-media-root={key} aria-pressed={!!listing&&(key==='output'?inOutput&&!inStarred&&!underRoot(listing.path,roots.input):key==='starred'?inStarred:underRoot(listing.path,roots.input))} onClick={()=>void open(path)}>{t(`media.root.${key}`)}</button>;})}</div>}

   <label className="media-depth"><span>{t('media.depth')}</span><select value={depth} onChange={e=>changeDepth(Number(e.target.value))}>{depths.map(d=><option key={d} value={d}>{d}</option>)}</select></label>
   {native&&roots.input&&<input ref={intake} type="text" readOnly className="media-intake" data-media-intake aria-label={t('media.intake.label')} placeholder={t('media.intake.label')} title={t('media.intake.hint')} value=""
    onDragOver={e=>{if(e.dataTransfer.types.includes(MEDIA_DRAG)){e.preventDefault();e.stopPropagation();}}}
    onDrop={e=>{const item=draggedItem(e.dataTransfer);if(!item)return;e.preventDefault();e.stopPropagation();setDragging(false);if(item.path)void copyIn([item.path]);else setIntakeStatus(t('media.intake.ignored'));}}/>}
   {intakeStatus&&<span className="media-intake-status" role="status">{intakeStatus}</span>}
   <span className="media-drop-hint">{t(native?'media.dropHelp':'media.browserHelp')}</span>
   <button type="button" disabled={!native} onClick={()=>void pick()}>{t('media.openFolder')}</button></header>
  <div className="media-browser">
   {listing&&<nav className="media-folders" aria-label={t('media.folders')}>
    <div className="media-path" aria-label={t('media.currentFolder')}><button type="button" disabled={!parent} aria-label={t('media.up')} onClick={()=>parent&&void open(parent)}>↑</button></div>
    <ul>{treeRoot&&folderBranch(treeRoot,rootName)}</ul>
   </nav>}
   <div className="media-files">
  {dropped.length>0&&<ul className="media-list" aria-label={t('media.dropped')}>{dropped.filter(i=>type==='all'||i.media===type).map((item,i)=><li key={`${item.src}-${i}`} data-mark={mark(item.path)}><button type="button" data-media={item.media} aria-current={selected?.src===item.src} draggable onDragStart={e=>dragItem(e.dataTransfer,item)} onContextMenu={onMenu(item)} onClick={()=>onSelect(item)}>{thumb(item.media,item.src)}<span className="media-name">{item.name}</span></button>{badge(item.path)}{canStar(item)&&<StarMediaButton item={item}/>}</li>)}</ul>}
  {error&&<p className="media-error" role="alert">{error}</p>}
  {listing&&<Tiles folder={listing.path} layout={view} className="media-list" aria-label={t('media.entries')} items={visible} render={entry=><li key={entry.path} data-mark={mark(entry.path)}><button type="button" data-kind={entry.kind} data-media={entry.media??undefined} aria-current={selected?.path===entry.path} title={entry.name} draggable onDragStart={e=>{const item=itemFromEntry(entry);if(item)dragItem(e.dataTransfer,item);}} onContextMenu={onMenu(itemFromEntry(entry))} onClick={()=>choose(entry)}>{native&&(entry.media==='image'||entry.media==='video')?<Thumb entry={entry}/>:thumb(entry.media,null)}<span className="media-name">{entry.name}{entry.symlink&&<span className="media-symlink" aria-label={t('media.symlink')}> ⤳</span>}</span></button>{badge(entry.path)}{canStar(itemFromEntry(entry)!)&&<StarMediaButton key={`${entry.path}:${outputRevision}`} item={itemFromEntry(entry)!}/>}</li>}/>}
  {menu&&<div className="media-menu" role="menu" aria-label={menu.item.name} style={{left:Math.min(menu.x,window.innerWidth-200),top:Math.min(menu.y,window.innerHeight-48*(1+menu.actions.length))}}>
   {menu.actions.map((a,i)=><button key={a.label} type="button" role="menuitem" autoFocus={i===0} disabled={!!a.blocked} title={a.blocked?undefined:a.hint} onClick={()=>{setMenu(null);a.onClick();}}>{a.label}{a.blocked&&<small>{a.blocked}</small>}</button>)}
   {native&&menu.item.path&&<button type="button" role="menuitem" autoFocus={!menu.actions.length} onClick={()=>reveal(menu.item.path!)}>{t('media.reveal')}</button>}</div>}
  {listing&&!visible.length&&!error?<p className="media-help">{t('media.empty')}</p>:listing?.truncated?<p className="media-help">{t('media.truncated')}</p>:null}
   </div>
  </div>
 </aside>;
}
