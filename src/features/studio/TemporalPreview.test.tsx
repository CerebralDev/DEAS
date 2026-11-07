import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { TemporalPreview } from './TemporalPreview';
import { nativeVideo } from './NativeVideo';
import type { TemporalPreviewData } from '../../lib/jobs';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:vi.fn(()=>true)}));
vi.mock('./NativeVideo',()=>({nativeVideo:vi.fn(()=>false),NativeVideo:vi.fn(({label,source,active,muted,loop})=><div aria-label={label} data-source={JSON.stringify(source)} data-active={active} data-muted={muted} data-loop={loop}/>)}));
const frames:TemporalPreviewData={revision:1,nodeId:'node',width:32,height:32,fps:8,frames:[{src:'data:image/png;base64,red',durationMs:125},{src:'data:image/png;base64,blue',durationMs:125}]};
const video:TemporalPreviewData={revision:1,nodeId:'node',width:32,height:32,fps:8,video:{previewJobId:'job',previewRevision:1}};
let draw:ReturnType<typeof vi.fn>, callbacks:FrameRequestCallback[];
beforeEach(()=>{
 vi.clearAllMocks(); vi.mocked(nativeVideo).mockReturnValue(false); vi.mocked(isTauri).mockReturnValue(true);
 draw=vi.fn();callbacks=[];
 vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({drawImage:draw} as unknown as CanvasRenderingContext2D);
 vi.stubGlobal('requestAnimationFrame',vi.fn((cb:FrameRequestCallback)=>{callbacks.push(cb);return callbacks.length;}));
 vi.stubGlobal('cancelAnimationFrame',vi.fn());
 vi.stubGlobal('Image',class {src='';complete=true;naturalWidth=32;onload=null;onerror=null;});
});
afterEach(()=>{cleanup();vi.unstubAllGlobals();vi.restoreAllMocks();});
describe('TemporalPreview',()=>{
 it('paints changing decoded frames with their duration and cancels inactive playback',async()=>{
  const {rerender}=render(<TemporalPreview preview={frames} active label="preview"/>);
  await waitFor(()=>expect(callbacks).toHaveLength(1));
  act(()=>callbacks.shift()!(0));act(()=>callbacks.shift()!(130));
  expect(draw.mock.calls.map(c=>c[0].src)).toEqual([frames.frames![0].src,frames.frames![1].src]);
  expect(screen.getByRole('img',{name:'preview'}).tagName).toBe('CANVAS');
  rerender(<TemporalPreview preview={frames} active={false} label="preview"/>);
  expect(cancelAnimationFrame).toHaveBeenCalled();
 });
 it('uses the native muted looping player on Linux, never HTML video',()=>{
  vi.mocked(nativeVideo).mockReturnValue(true);
  const {container}=render(<TemporalPreview preview={video} active label="preview"/>);
  expect(container.querySelector('video')).toBeNull();
  expect(screen.getByLabelText('preview').getAttribute('data-muted')).toBe('true');
  expect(screen.getByLabelText('preview').getAttribute('data-source')).toBe(JSON.stringify(video.video));
  expect(invoke).not.toHaveBeenCalled();
 });
 it('loads an opaque Windows preview and drops it when inactive',async()=>{
  vi.mocked(invoke).mockResolvedValue('data:video/mp4;base64,fixture');
  const {container,rerender}=render(<TemporalPreview preview={video} active label="preview"/>);
  await waitFor(()=>expect(container.querySelector('video')).not.toBeNull());
  const player=container.querySelector('video')!;expect(player.muted).toBe(true);expect(player.loop).toBe(true);
  expect(invoke).toHaveBeenCalledWith('native_video_open',expect.objectContaining({source:video.video,muted:true,repeat:true}));
  rerender(<TemporalPreview preview={video} active={false} label="preview"/>);expect(container.querySelector('video')).toBeNull();
 });
 it('falls back on a failed open, reports through the typed shared-log seam and recovers on the next revision',async()=>{
  vi.mocked(invoke).mockImplementation(async(command)=>{if(command==='native_video_open')throw new Error('decoder failed');});
  const {container,rerender}=render(<TemporalPreview preview={video} active label="preview" fallback="data:image/jpeg;base64,fallback"/>);
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_video_update',expect.objectContaining({action:{kind:'previewError'}})));
  expect(screen.getByRole('img',{name:'preview'}).tagName).toBe('IMG');
  rerender(<TemporalPreview preview={{...frames,revision:2}} active label="preview"/>);
  await waitFor(()=>expect(callbacks).toHaveLength(1));act(()=>callbacks.shift()!(0));
  expect(container.querySelector('canvas')).not.toBeNull();expect(draw).toHaveBeenCalled();
 });
 it('ignores an obsolete open result after a revision change',async()=>{
  let settle!:(value:string)=>void;
  vi.mocked(invoke).mockImplementationOnce(()=>new Promise(resolve=>{settle=resolve as typeof settle;})).mockResolvedValue('data:video/mp4;base64,new');
  const {container,rerender}=render(<TemporalPreview preview={video} active label="preview"/>);
  rerender(<TemporalPreview preview={{...video,revision:2,video:{previewJobId:'job',previewRevision:2}}} active label="preview"/>);
  await waitFor(()=>expect(container.querySelector('video')?.getAttribute('src')).toContain('new'));
  await act(async()=>settle('data:video/mp4;base64,old'));
  expect(container.querySelector('video')?.getAttribute('src')).toContain('new');
 });
});
