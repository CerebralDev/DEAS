"""Audio enhancement UI acceptance; strict simulated IPC, no native processing.

Run: uv run --no-project --with playwright==1.64.1 python tools/verify_audio_enhance.py
Native FFmpeg acceptance is the separate ignored Rust audio_real_ffmpeg test.
"""
import json
from playwright.sync_api import expect
import verify_postprocessing as runner
import verify_workbench_collection as collection
import verify_workspace_layout as layout

FIXTURE = layout.FIXTURE + r'''
const audioInvoke=window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke=async(command,args)=>{
 if(command==='native_audio_enhance_capabilities'){
  f.calls.push({command,args:clone(args??{})});
  return {clean:true,deep:false,stems:false};
 }
 if(command==='native_audio_enhance_export'){
  f.calls.push({command,args:clone(args??{})});
  const r=args?.request;
  if(!r||r.sourceId!==window.__videoSource.sourceId||r.mode!=='clean'||r.profile!=='mix'||r.confirmed!==true||Object.keys(r).sort().join(',')!=='confirmed,mode,profile,sourceId,title')return fail('unsafe audio export payload');
  return {path:'/fixture/export/enhanced.mkv',size:42,sha256:'a'.repeat(64)};
 }
 return audioInvoke(command,args);
};
'''


def verify(browser, url, shots):
    original = collection.FIXTURE
    collection.FIXTURE += FIXTURE
    try:
        context, page, errors = collection.open_studio(browser, url, False)
    finally:
        collection.FIXTURE = original
    try:
        page.locator('[data-studio-tab="tve"]').click()
        card = page.locator('[data-audio-enhance="tve"]')
        expect(card).to_be_visible()
        mode = card.locator('[data-audio-enhance-mode]')
        expect(mode).to_have_value('off')
        page.locator('[data-video-post-inspect]').click()
        expect(card.locator('[data-audio-enhance-source]')).to_have_text('clip 猫.mp4')
        mode.focus()
        expect(mode.locator('option[value="deep"]')).to_have_js_property('disabled', True)
        expect(mode.locator('option[value="stems"]')).to_have_js_property('disabled', True)
        mode.select_option('clean')
        card.locator('[data-audio-enhance-run]').click()
        expect(card.locator('[data-audio-enhance-saved]')).to_contain_text('/fixture/export/enhanced.mkv')
        for width, height in [(1440, 960), (390, 844), (320, 640)]:
            page.set_viewport_size({'width': width, 'height': height})
            card.locator('[data-audio-enhance-run]').scroll_into_view_if_needed()
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth+1')
            box = card.locator('[data-audio-enhance-run]').bounding_box()
            assert box and box['y'] >= 0 and box['y']+box['height'] <= height
            page.screenshot(path=str(shots/f'audio-enhance-{width}.png'))
        calls = page.evaluate('__collection.calls')
        assert len([v for v in calls if v['command']=='native_audio_enhance_export']) == 1
        assert not [v for v in calls if v['command'] in ('native_video_post_upload','native_workbench_run')]
        collection.assert_clean(page, errors)
        print(json.dumps({'status':'PASS','kind':'Chromium; simulated IPC only','checks':['off by default','retained TVE source','missing tiers disabled','nested native request','audio-only without backend job','new-copy receipt','desktop/narrow reachability'],'screenshots':str(shots)},indent=2))
    finally:
        context.close()


if __name__ == '__main__':
    runner.verify = verify
    runner.main()
