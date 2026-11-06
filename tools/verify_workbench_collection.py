"""Bounded Studio collection acceptance using isolated, strict simulated native IPC only.

Run against an already running loopback Vite server:
  python tools/verify_workbench_collection.py http://127.0.0.1:PORT
No backend, models, installs, browser dependencies, or repository screenshots are written.
"""
import argparse
import json
import os
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import expect, sync_playwright


def set_locale(page, locale, back):
    page.locator('[data-node="settings"]').click()
    page.locator('#language-select').select_option(locale)
    page.locator(back).click()


FIXTURE = r"""
localStorage.setItem('deas.pref.interface.experience', JSON.stringify('advanced'));
window.isTauri=true;
window.__collection={calls:[],unexpected:[],paths:[],polls:0,release:0};
const f=window.__collection;
const png='data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mO8+e7HfwAJHgPpUpmX6wAAAABJRU5ErkJggg==';
const clone=x=>structuredClone(x);
const freeze=x=>{if(x&&typeof x==='object'){Object.values(x).forEach(freeze);Object.freeze(x);}return x;};
const intent=(name,seed)=>freeze({recipeId:'checkpoint-text-to-image-diagnostic',version:1,
 positivePrompt:'Submitted '+name+' — forest 猫',negativePrompt:'Original negative '+name,
 checkpoint:'sdxl/original-'+name+'.safetensors',seed,width:768,height:512,steps:17,cfg:4.5,
 sampler:'euler',scheduler:'normal',outputPrefix:'held/'+name,
 loras:[{name:'style/held.safetensors',strength:0.75}]});
const output=name=>({filename:name,subfolder:'original/subfolder',type:'output'});
const job=(id,state,seed,outputs=[])=>({id,baseUrl:'http://127.0.0.1:'+seed+'/',
 promptId:'prompt-'+id,intent:intent(id,seed),appVersion:'0.1.45',
 metadata:{startedAt:1700000000000,generationMs:state==='succeeded'?13310:null},
 state,outputs,error:state==='unknown'?'Fixture observation unavailable':null,generationReady:false});
f.jobs=__MIXED__?[
 job('queued01-held','queued',7901),job('running2-held','running',7902),
 job('unknown3-held','unknown',7903,[output('unknown-kept.png')]),
 job('success4-held','succeeded',7904,[output('original-A.png'),output('original-B.png')])]:[];
const snapshot=()=>clone({jobs:f.jobs,preparing:0,capacity:20,stopAvailable:true,admissionEpoch:0,stopping:false});
const fail=message=>{f.unexpected.push(message);throw Error(message);};
window.__TAURI_INTERNALS__={convertFileSrc:path=>{f.paths.push(path);return png;},invoke:async(command,args)=>{
 f.calls.push({command,args:clone(args??{})});
 if(command==='native_workbench_current'){
  if(Object.keys(args??{}).length)return fail('current must have empty args');
  return snapshot();
 }
 if(command==='native_workbench_poll'){
  if(Object.keys(args??{}).length)return fail('collection poll must not take endpoint/job args');
  f.polls++;
  f.jobs=f.jobs.map(j=>{
   if(j.state!=='queued'&&j.state!=='running')return j;
   if(f.release>=2)return {...j,state:'succeeded',outputs:[output(j.id+'-result.png')],metadata:{...j.metadata,generationMs:13310}};
   if(f.release>=1)return {...j,state:'running',progress:{value:9,max:17}};
   return j;
  });
  return snapshot();
 }
 if(command==='native_workbench_run'){
  if(__MIXED__||f.calls.filter(c=>c.command===command).length!==1)return fail('unexpected/repeated Run');
  if(args.confirmed!==true||args.admissionEpoch!==0||args.baseUrl!=='http://127.0.0.1:7801'||!args.intent?.positivePrompt)return fail('invalid explicit run');
  const submitted=job('newrun01-held','queued',7801);
  submitted.baseUrl=args.baseUrl+'/';submitted.intent=freeze(clone(args.intent));
  f.jobs.push(submitted);return clone(submitted);
 }
 if(command==='native_job_output'){
  const held=f.jobs.find(j=>j.id===args.jobId);
  if(!held||held.state!=='succeeded'||!held.outputs[args.index])return fail('invalid output owner/index');
  return {jobId:args.jobId,index:args.index,mimeType:'image/png',width:1,height:1,sha256:'0'.repeat(64),bytes:Array.from(atob(png.split(',')[1]),c=>c.charCodeAt(0))};
 }
 if(command==='native_recipe_prerequisites')return {baseUrl:args.baseUrl.replace(/\/$/,'')+'/',recipeId:'checkpoint-text-to-image-diagnostic',version:1,
  allowlist:{nodes:['CheckpointLoaderSimple','CLIPTextEncode','EmptyLatentImage','KSampler','VAEDecode','SaveImage'],checkpoints:['sdxl/fresh.safetensors'],loras:[],samplers:['euler'],schedulers:['normal']},missingNodes:[],generationReady:false};
 if(command==='native_media_star_status')return {starred:false,unavailable:null};
 if(command==='native_media_list')return {path:args.path||'/fixture-unused',parent:null,entries:[],truncated:false};
 if(command==='native_install_plan')return {id:'fixture-plan',request:args.request,steps:[],warnings:[],unsupportedOptions:[],canExecute:true};
 if(command==='native_startup_phase'||command==='native_startup_ready')return null;
 if(command==='native_prompt_spellcheck')return null;
 if(command==='native_spellcheck_status')return {available:true,installed:true};
 if(command==='native_wildcard_library')return JSON.stringify({categories:[]});
 if(command==='native_auth_status')return [];
 if(command==='native_update_check')return {app:null,comfy:false,nodepack:false};
 if(command==='native_repo_stars')return {count:0,starred:null};
 if(command==='native_install_defaults')return {storagePath:'/fixture-unused',backend:'cpu',warnings:[]};
 return fail('Unexpected command '+command);
}};
"""

ROWS = '.wb-queue .wb-queue-select[data-state]'
FORBIDDEN = ('native_job_prepare', 'native_job_submit', 'native_job_status', 'native_job_current')
LOCALES = ('de', 'en', 'fr', 'ja', 'tr', 'zh')


def row(page, identifier):
    # The real queue exposes the first eight ID characters, not a test-owned state hook.
    return page.locator(ROWS).filter(has_text=identifier[:8]).first


def assert_clean(page, errors):
    assert not errors, errors
    assert page.evaluate('window.__collection.unexpected') == [], page.evaluate('window.__collection.unexpected')
    forbidden = page.evaluate('(names)=>__collection.calls.filter(c=>names.includes(c.command))', list(FORBIDDEN))
    assert not forbidden, forbidden


def no_automatic_stop(page):
    assert page.evaluate('__collection.calls.filter(c=>c.command==="native_workbench_stop").length') == 0


def open_studio(browser, url, mixed, width=1440, rooted=False):
    context = browser.new_context(viewport={'width': width, 'height': 960}, reduced_motion='reduce')
    page = context.new_page()
    page.set_default_timeout(8000)
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    # Block accidental backend/external traffic even if a future component bypasses native IPC.
    origin = urlparse(url).netloc
    def route(request):
        target = urlparse(request.request.url)
        if target.scheme in ('http', 'https') and target.netloc != origin:
            errors.append('Unexpected network request '+request.request.url)
            request.abort()
        else:
            request.continue_()
    page.route('**/*', route)
    script = FIXTURE.replace('__MIXED__', 'true' if mixed else 'false')
    if rooted:
        script += "\n__collection.jobs.find(j=>j.id==='success4-held').outputRoot='/captured/old/output';"
    page.add_init_script(script)
    page.goto(url, wait_until='networkidle', timeout=30000)
    try:
        set_locale(page, 'en', '[data-node="studio"]')
    except Exception:
        print('STARTUP_DIAGNOSTICS', {'errors': errors, 'page': page.evaluate('({text:document.body.innerText,calls:window.__collection?.calls,unexpected:window.__collection?.unexpected})')}, flush=True)
        raise
    expect(page.locator('[data-workbench]')).to_be_visible()
    page.wait_for_function('__collection.calls.some(c=>c.command==="native_workbench_current")')
    return context, page, errors


def load_model(page):
    page.locator('[data-dock-tab="models"]').click()
    page.locator('[data-run-load]').click()
    expect(page.locator('[data-model-picker]')).to_have_value('sdxl/fresh.safetensors')


def metadata(page, identifier):
    row(page, identifier).click()
    expect(row(page, identifier)).to_have_attribute('aria-current', 'true')
    toggle = page.locator('[data-metadata-toggle]')
    if toggle.get_attribute('aria-pressed') != 'true':
        toggle.click()
    panel = page.locator('.wb-metadata')
    expect(panel).to_be_visible()
    return panel


def submitted_metadata(page, identifier, seed):
    panel = metadata(page, identifier)
    for value in ('Submitted '+identifier+' — forest 猫', 'Original negative '+identifier,
                  'sdxl/original-'+identifier+'.safetensors', str(seed), '768 × 512',
                  '17', '4.5', 'euler', 'normal', 'style/held.safetensors (0.75)'):
        expect(panel.locator('dd').get_by_text(value, exact=True)).to_have_count(1)
    return panel


def mixed_case(browser, url, screenshots):
    context, page, errors = open_studio(browser, url, True)
    try:
        expect(page.locator(ROWS)).to_have_count(5)
        for identifier, state in (('queued01-held', 'queued'), ('running2-held', 'running'),
                                  ('unknown3-held', 'unknown'), ('success4-held', 'succeeded')):
            expect(row(page, identifier)).to_have_attribute('data-state', state)
        assert page.evaluate('__collection.calls.filter(c=>c.command==="native_workbench_run").length') == 0
        load_model(page)  # Explicit repeated Run is available once the current draft validates.
        page.locator('[data-run-positive]').fill('Editable draft, not any held intent')
        expect(page.locator('[data-run]')).to_be_enabled()
        no_automatic_stop(page)
        submitted_metadata(page, 'unknown3-held', 7903)
        submitted_metadata(page, 'success4-held', 7904)
        expect(page.locator('[data-job-output-preview]')).to_be_visible()
        for name in ('original-A.png', 'original-B.png', 'unknown-kept.png'):
            expect(page.locator('.wb-queue').get_by_text(name, exact=True)).to_have_count(1)
        submitted_metadata(page, 'running2-held', 7902)
        page.locator('[data-run-positive]').fill('Edited after recovery')
        page.get_by_role('spinbutton', name='Seed', exact=True).fill('123')
        page.get_by_role('spinbutton', name='Steps', exact=True).fill('28')
        page.evaluate('__collection.release=1')
        expect(row(page, 'queued01-held')).to_have_attribute('data-state', 'running')
        expect(row(page, 'running2-held')).to_contain_text('9/17')
        expect(row(page, 'running2-held')).to_have_attribute('aria-current', 'true')
        expect(page.locator('.wb-metadata').get_by_text('Submitted running2-held — forest 猫', exact=True)).to_be_visible()
        page.evaluate('__collection.release=2')
        for identifier in ('queued01-held', 'running2-held'):
            expect(row(page, identifier)).to_have_attribute('data-state', 'succeeded')
            expect(page.locator('.wb-queue').get_by_text(identifier+'-result.png', exact=True)).to_have_count(1)
        expect(row(page, 'running2-held')).to_have_attribute('aria-current', 'true')
        expect(page.locator('[data-job-output-preview]')).to_be_visible()
        expect(page.locator(ROWS)).to_have_count(5)
        expect(row(page, 'unknown3-held')).to_have_attribute('data-state', 'unknown')
        expect(page.locator('[data-run]')).to_be_enabled()  # Retained unknowns never trigger an automatic retry.
        submitted_metadata(page, 'running2-held', 7902)
        submitted_metadata(page, 'success4-held', 7904)
        for name in ('original-A.png', 'original-B.png', 'unknown-kept.png'):
            expect(page.locator('.wb-queue').get_by_text(name, exact=True)).to_have_count(1)
        assert page.evaluate('__collection.polls') >= 2
        assert page.evaluate('__collection.calls.filter(c=>c.command==="native_workbench_run").length') == 0
        assert_clean(page, errors)
        print('PASS recovery + collection progression + immutable metadata/output retention', flush=True)
        count = 1
        for width in (1440, 320):
            page.set_viewport_size({'width': width, 'height': 960})
            for locale in LOCALES:
                set_locale(page, locale, '[data-node="studio"]')
                expect(page.locator('[data-workbench]')).to_be_visible()
                # Let resize/locale layout effects settle without changing fixture state.
                page.evaluate('()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))')
                geometry = page.evaluate('({width:innerWidth,scroll:document.documentElement.scrollWidth})')
                assert geometry['scroll'] <= geometry['width'], (width, locale, geometry)
                expect(page.locator(ROWS)).to_have_count(5)
                expect(page.locator('[data-run]')).to_be_enabled()
                if locale == 'en':
                    shot = screenshots / f'workbench-collection-{width}.png'
                    page.screenshot(path=str(shot), full_page=True)
                    print('SCREENSHOT '+str(shot), flush=True)
                assert_clean(page, errors)
                print(f'PASS layout {width} {locale}', flush=True)
                count += 1
        return count
    finally:
        context.close()


def empty_case(browser, url):
    context, page, errors = open_studio(browser, url, False)
    try:
        expect(page.locator(ROWS)).to_have_count(0)
        assert page.evaluate('__collection.calls.filter(c=>c.command==="native_workbench_run").length') == 0
        load_model(page)
        page.locator('[data-run-positive]').fill('Explicit collection submission — copper forest')
        page.locator('[data-run-negative]').fill('Original new-run negative')
        page.get_by_role('spinbutton', name='Seed', exact=True).fill('42')
        expect(page.locator('[data-run]')).to_be_enabled()
        page.locator('[data-run]').click()
        expect(row(page, 'newrun01-held')).to_have_attribute('data-state', 'queued')
        expect(page.locator('[data-run]')).to_be_enabled()
        runs = page.evaluate('__collection.calls.filter(c=>c.command==="native_workbench_run")')
        assert len(runs) == 1 and runs[0]['args']['confirmed'] is True, runs
        intent = runs[0]['args']['intent']
        assert intent['seed'] == 42 and intent['checkpoint'] == 'sdxl/fresh.safetensors', intent
        panel = metadata(page, 'newrun01-held')
        page.locator('[data-run-positive]').fill('Changed after explicit Run')
        expect(panel.get_by_text(intent['positivePrompt'], exact=True)).to_be_visible()
        page.evaluate('__collection.release=1')
        expect(row(page, 'newrun01-held')).to_have_attribute('data-state', 'running')
        expect(page.locator('[data-run]')).to_be_enabled()
        no_automatic_stop(page)
        page.evaluate('__collection.release=2')
        expect(row(page, 'newrun01-held')).to_have_attribute('data-state', 'succeeded')
        expect(page.locator('[data-job-output-preview]')).to_be_visible()
        expect(row(page, 'newrun01-held')).to_contain_text('newrun01-held-result.png')
        expect(panel.get_by_text(intent['positivePrompt'], exact=True)).to_be_visible()
        assert page.evaluate('__collection.calls.filter(c=>c.command==="native_workbench_run").length') == 1
        assert_clean(page, errors)
        print('PASS empty recovery + one explicit Run + collection result', flush=True)
        return 1
    finally:
        context.close()


def rooted_case(browser, url):
    context, page, errors = open_studio(browser, url, True, rooted=True)
    try:
        metadata(page, 'success4-held')
        expected = ['/captured/old/output/original/subfolder/'+name
                    for name in ('original-A.png', 'original-B.png')]
        expect(page.locator('[data-job-id="success4-held"] img')).to_have_count(2)
        page.wait_for_function('(paths)=>paths.every(p=>__collection.paths.includes(p))', arg=expected)
        page.wait_for_function('(paths)=>paths.every(p=>__collection.calls.some(c=>c.command==="native_media_star_status"&&c.args.path===p))', arg=expected)
        assert not page.evaluate('__collection.calls.some(c=>c.command==="native_job_output")')
        assert_clean(page, errors)
        print('PASS recovered local output paths use captured per-job root', flush=True)
        return 1
    finally:
        context.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('url', help='Existing loopback Vite URL; this script never starts/stops servers')
    args = parser.parse_args()
    parsed = urlparse(args.url)
    if parsed.scheme != 'http' or parsed.hostname not in ('127.0.0.1', 'localhost', '::1'):
        parser.error('an explicit HTTP loopback URL is required')
    scratch = Path(os.environ.get('TMPDIR', str(Path.home()/'.hermes/cache/scratch'))).resolve()
    # Always keep artifacts under Hermes scratch, even if an unrelated TMPDIR is exported.
    hermes_scratch = (Path.home()/'.hermes/cache/scratch').resolve()
    if not scratch.is_relative_to(hermes_scratch):
        scratch = hermes_scratch
    import tempfile
    scratch.mkdir(parents=True, exist_ok=True)
    screenshots = Path(tempfile.mkdtemp(prefix='deas-collection-', dir=scratch))
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        try:
            count = mixed_case(browser, args.url, screenshots) + empty_case(browser, args.url) + rooted_case(browser, args.url)
        finally:
            browser.close()
    print(json.dumps({'passed': count, 'kind': 'simulated native IPC only', 'screenshots': str(screenshots)}))


if __name__ == '__main__':
    main()
