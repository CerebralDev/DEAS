"""Viewport contract regression: real browser geometry, read-only IPC fixtures only.

Run: uv run --no-project --with playwright python tools/verify_workspace_layout.py
Owns its Vite/browser children; never accesses real models, media or backend services.
"""
import json
from pathlib import Path
from playwright.sync_api import expect
import verify_postprocessing as runner
import verify_workbench_collection as collection
import verify_video_post as video

GEOMETRY = r'''()=>{
 const rect=e=>{const r=e.getBoundingClientRect();return {top:r.top,bottom:r.bottom,width:r.width,height:r.height}};
 const active=[...document.querySelectorAll('.studio-generate-host:not([hidden])')].find(e=>e.getClientRects().length);
 return {height:innerHeight,width:innerWidth,scrollHeight:document.documentElement.scrollHeight,scrollWidth:document.documentElement.scrollWidth,
  shell:rect(document.querySelector('.studio-shell')),frame:rect(document.querySelector('.aio-decision-area')),left:rect(document.querySelector('.aio-left')),panes:[...document.querySelectorAll('.aio-left,.aio-right')].filter(e=>e.getClientRects().length).map(rect),
  active:active?rect(active):null,activeOnly:active&&![...active.parentElement.children].some(e=>e!==active&&e.getClientRects().length),
  hidden:[...document.querySelectorAll('[hidden]:not([hidden="until-found"])')].filter(e=>e.getClientRects().length).map(e=>e.className||e.tagName),
  footer:{...rect(document.querySelector('.footer')),position:getComputedStyle(document.querySelector('.footer')).position}};
}'''


def assert_viewport(page, label):
    g = page.evaluate(GEOMETRY)
    assert abs(max(p['bottom'] for p in g['panes']) - g['frame']['bottom']) <= 2, (label, 'viewport chain shrinks and leaves footer-only space', g)
    assert abs(g['shell']['height'] - g['height']) <= 2, (label, 'shell must own one viewport', g)
    assert g['scrollHeight'] <= g['height'] + 2, (label, 'document scroll; content must scroll inside its pane', g)
    assert g['scrollWidth'] <= g['width'] + 1, (label, 'horizontal page overflow', g)
    assert not g['hidden'], (label, 'retained hidden panes occupy layout', g)
    assert g['footer']['position'] == 'fixed' and g['footer']['height'] < 24, (label, 'version reserves workspace space', g)
    if g['active'] and g['activeOnly']:
        assert abs(g['active']['bottom'] - g['frame']['bottom']) <= 2, (label, 'Studio tab does not inherit viewport', g)
    if page.locator('.aio-utility-tabs').count():
        utility = page.locator('.aio-utility-tabs')
        assert utility.locator('[role="tab"]').evaluate_all('(es)=>es.map(e=>e.dataset.studioTab||e.dataset.node)') == ['monitor','map','settings','logs'], label
        creative = page.locator('.aio-topbar > .aio-tabs').first
        assert creative.locator('[role="tab"]').evaluate_all('(es)=>es.map(e=>e.dataset.studioTab||e.dataset.node)')[:7] == ['generate','optional-downloads','forge','chat','analysis','tve','comfy'], label
        if g['width'] >= 1440:
            left, right = creative.bounding_box(), utility.bounding_box()
            assert left['x']+left['width'] <= right['x']+1, (label,'utility group is not right of creative tabs')
    return g


FIXTURE = video.IPC + r'''
localStorage.setItem('deas.pref.install.root', JSON.stringify('/fixture/env'));
localStorage.setItem('deas.pref.backend.autostart', 'false');
window.__TAURI_INTERNALS__.metadata={currentWindow:{label:'main'},currentWebview:{label:'main'}};
window.__TAURI_INTERNALS__.transformCallback=()=>1;
window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
const layoutInvoke=window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke=async(c,a)=>{
 if(c==='plugin:event|listen'||c==='plugin:event|unlisten')return 1;
 const read={
  native_model_library_read:{schemaVersion:1,revision:0,folders:[],entries:{},order:[],views:[]},native_model_assets:{},native_model_scan_status:{id:"",root:"",state:"idle",completed:0,total:0,failed:0,remaining:0,file:null},native_files_present:[true],native_install_open:{state:'succeeded',logs:[],error:null},
  native_settings_read:{},native_models_scan:window.__workspaceModels??[],
  native_theme_packs:{directory:'/fixture/env/themes',packs:[],warnings:[]},
  native_llm_read:{provider:'openai',baseUrl:'http://127.0.0.1:49312'},native_llm_models:{models:[{id:'openai:layout-fixture',provider:'openai',name:'Layout fixture'}],warnings:[]},
  native_forge_randomizer_list:[],native_forge_targets:{source:{commit:'layout-fixture'},creativity:['balanced'],targets:[{key:'illustrious',label:'Layout fixture',modes:[],defaultMode:null,optional:[],qualityPresets:[],styles:[],aspectRatios:null,hasNegative:true,tools:false,guidance:[]}]},
  native_startup_read:{startup:{preset:'off',flags:[],fast:[]},packages:[],presets:{speed:{flags:[],fast:[]},compatibility:{flags:[],fast:[]},off:{flags:[],fast:[]}}},
  native_backend_status:{state:'stopped',endpoint:null,logs:[],error:null,generationReady:false},
  native_analysis_overview:{runtimeReady:false,install:{running:false,stage:"",downloaded:0,total:0,error:null},taggers:[],unusable:[],defaultSize:0,runtimeSize:0,toolsSize:0},native_chat_list:[],native_chat_settings:{search:'off',searxngUrl:''},native_logs:[],
  native_spellcheck_status:{available:true,installed:true},native_spellcheck_check:[],
  native_jobs_history:{schemaVersion:1,records:[],readOnly:false,error:null},
  native_wildcard_library:JSON.stringify({categories:[]}),
  native_audio_enhance_install_status:{running:false,mode:"",stage:"",downloaded:0,total:0},
  native_audio_enhance_capabilities:{clean:true,deep:false,stems:false},
  native_monitoring_sample:{at:0,source:'layout-fixture',cpu:null,ram:null,swap:null,disks:[],diskIo:null,gpus:[]}
 };
 if(c==='native_install_plan'){f.calls.push({command:c,args:clone(a??{})});return {id:'layout-reopen',reopenId:'checked',request:a.request,steps:[],warnings:[],unsupportedOptions:[],canExecute:true};}
 if(Object.hasOwn(read,c)){f.calls.push({command:c,args:clone(a??{})});return clone(read[c]);}
 return layoutInvoke(c,a);
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
        checks = []
        picker = page.locator('[data-model-picker]')
        expect(picker).to_be_visible()
        assert picker.evaluate("e=>e.tagName==='SELECT'&&!e.multiple&&e.size===0")
        assert page.locator('[data-model-filter], [data-model-list]').count() == 0
        page.screenshot(path=str(shots/'checkpoint-native-desktop.png'))
        page.locator('[data-run-positive]').fill('Viewport draft retained')
        tabs = page.locator('.aio-tabs [role="tab"]').evaluate_all("els=>els.map(e=>({studio:e.dataset.studioTab,node:e.dataset.node}))")
        assert tabs
        for locale in collection.LOCALES:
            collection.set_locale(page, locale, '[data-node="studio"]')
            for width, height in [(1920,1080),(1440,960),(1024,768),(768,900),(390,844),(320,640),(1440,420)]:
                page.set_viewport_size({'width':width,'height':height})
                for target in tabs:
                    selector = '[data-studio-tab="'+target['studio']+'"]' if target.get('studio') else '[data-node="'+target['node']+'"]'
                    button = page.locator('.aio-tabs '+selector)
                    button.click()
                    try:
                        expect(button).to_have_attribute('aria-selected', 'true')
                    except AssertionError:
                        print('BROWSER_ERRORS', errors, flush=True)
                        print('NAVIGATION_DIAGNOSTICS', page.evaluate('({calls:__collection.calls,unexpected:__collection.unexpected,text:document.body.innerText.slice(0,6000)})'), flush=True)
                        raise
                    page.wait_for_timeout(35)
                    label = f'{locale}:{width}x{height}:{target}'
                    try:
                        assert_viewport(page, label)
                    except Exception:
                        print('FAILED_LAYOUT',label,'BROWSER_ERRORS',errors,flush=True)
                        print(page.locator('body').inner_text()[:2000],flush=True)
                        raise
                    checks.append(label)
                page.locator('[data-studio-tab="generate"]').click()
                assert_viewport(page, 'return to retained Generate')
                expect(page.locator('[data-run-positive]')).to_have_value('Viewport draft retained')
            if locale in ('de','en'):
                page.locator('[data-studio-tab="tve"]').click()
                audio = page.locator('[data-audio-enhance="tve"]')
                expect(audio).to_be_visible()
                audio.locator('[data-audio-enhance-run]').scroll_into_view_if_needed()
                assert audio.locator('[data-audio-enhance-mode]').input_value() == 'off'
                assert audio.locator('[data-audio-enhance-run]').is_disabled()
                checks.append(f'{locale}:TVE audio enhancement reachable and opt-in OFF')
                page.set_viewport_size({'width':1440,'height':960})
                page.screenshot(path=str(shots/f'workspace-{locale}-desktop.png'))
                page.set_viewport_size({'width':390,'height':844})
                page.screenshot(path=str(shots/f'workspace-{locale}-narrow.png'))
        collection.set_locale(page,'en','[data-node="studio"]')
        page.set_viewport_size({'width':1440,'height':960})
        if (Path(__file__).resolve().parent.parent/'src/features/workspace/Docking.tsx').exists():
            page.locator('[data-studio-tab="chat"]').click()
            page.locator('[data-chat-input]').fill('Dock draft survives every move')
            page.evaluate("window.__chatNode=document.querySelector('[data-chat-input]')")
            page.get_by_label('Dock monitoring',exact=True).check()
            for position in ['left','right','bottom','full']:
                page.get_by_label('Chat placement',exact=True).select_option(position)
                for width,height in [(1440,960),(390,844),(1440,420)]:
                    page.set_viewport_size({'width':width,'height':height})
                    assert_viewport(page,f'docks:{position}:{width}x{height}')
                    assert page.evaluate("window.__chatNode===document.querySelector('[data-chat-input]')")
                    expect(page.locator('[data-chat-input]')).to_have_value('Dock draft survives every move')
                    assert page.locator('[data-monitoring]').count()==1
                    checks.append(f'docks:{position}:{width}x{height}:single session + feed')
            page.get_by_role('button',name='Reset workspace layout',exact=True).click()
        if (Path(__file__).resolve().parent.parent/'src/components/models/ModelLibrary.tsx').exists():
            page.evaluate("window.__workspaceModels=Array.from({length:600},(_,i)=>({category:'loras',name:'Group '+(i%20)+'/Long descriptive resource '+i+'.safetensors',path:'/fixture/models/loras/model-'+i+'.safetensors',size:1024+i,info:null,family:'minimax-h3'}))")
            page.locator('[data-node="optional-downloads"]').click()
            page.locator('[data-library-tab="installed"]').click()
            expect(page.locator('.download-card')).to_have_count(600)
            for width,height in [(1440,960),(390,844)]:
                page.set_viewport_size({'width':width,'height':height})
                for hierarchy in ['disk','auto','manual']:
                    page.get_by_label('Hierarchy',exact=True).select_option(hierarchy)
                    assert_viewport(page,f'large-library:{hierarchy}:{width}')
                    checks.append(f'large-library:{hierarchy}:{width}:600 resources')
            page.screenshot(path=str(shots/'library-organized-narrow.png'))
        page.set_viewport_size({'width':1440,'height':960})
        page.locator('[data-studio-tab="tve"]').click()
        page.evaluate("""()=>{
          document.querySelector('.studio-generate-host:not([hidden])').hidden=true;
          const host=document.createElement('div');host.className='studio-generate-host';host.dataset.futureHost='';
          host.innerHTML='<section class="future-pane"><input aria-label="future first"></section>';
          document.querySelector('.studio-main').append(host);
        }""")
        assert_viewport(page, 'unknown future tab inherits shared viewport')
        checks.append('future tab without feature-specific height rules')
        page.evaluate("""()=>{const e=document.querySelector('[data-future-host]');e.style.flex='none';e.style.height='40px';}""")
        try:
            assert_viewport(page, 'deliberately broken tab')
        except AssertionError:
            checks.append('negative control: footer-only gap is rejected')
        else:
            raise AssertionError('layout guard accepted a deliberately shrunken pane')
        page.evaluate("""()=>{
          const host=document.querySelector('[data-future-host]');host.removeAttribute('style');
          host.firstElementChild.innerHTML='<div style="height:2000px">Long future content</div><input aria-label="future last">';
          const style=document.createElement('style');style.textContent='.studio-generate-host[hidden]{display:grid}';document.head.append(style);
        }""")
        page.get_by_role('textbox',name='future last').scroll_into_view_if_needed()
        box = page.get_by_role('textbox',name='future last').bounding_box()
        assert box and box['y'] >= 0 and box['y'] + box['height'] <= page.viewport_size['height']
        assert_viewport(page, 'long future tab scrolls inside viewport; retained hidden CSS cannot override hiding')
        checks.append('long future content reachable without page scroll or hidden-pane leakage')
        collection.assert_clean(page, errors)
        forbidden=page.evaluate("__collection.calls.filter(c=>/(_start|_stop|_write|_send|_generate|_run|_submit|_cancel)$/.test(c.command))")
        assert not forbidden, forbidden
        result={'status':'PASS','checksPassed':len(checks),'tabCount':len(tabs),'checks':checks,'kind':'Chromium geometry; strict simulated IPC; no native services','screenshots':str(shots)}
        (shots/'result.json').write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps(result,indent=2))
    finally:
        context.close()


if __name__ == '__main__':
    runner.verify = verify
    runner.main()
