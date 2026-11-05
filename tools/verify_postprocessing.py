"""Chromium paired-output acceptance with synthetic media and strict simulated Tauri IPC.

Run: uv run --no-project --with playwright python tools/verify_postprocessing.py
Owns one isolated loopback Vite child; never contacts native/backend/model services.
Reuses the collection fixture and browser helpers; artifacts stay in Hermes scratch.
"""
import json
from pathlib import Path
import socket
import subprocess
import tempfile
import time
from urllib.request import urlopen

from playwright.sync_api import expect, sync_playwright
import verify_workbench_collection as collection


# Parameterize the existing fixture before its collection snapshot is created.
JOBS = r"""
const paired=(id,state,rooted=true)=>({...job(id,state,7904,[
 {...output(id+'-final.png'),role:'processed'},
 {...output(id+'-baseline.png'),role:'original'}]),
 ...(rooted?{outputRoot:'/captured/'+id}:{}),
 error:state==='failed'?'Synthetic post-processing failure':null});
f.jobs=[paired('paired01-held','succeeded'),paired('cancel02-held','cancelled'),
 paired('failed03-held','failed'),paired('rootless-held','succeeded',false),
 paired('rootcancel-held','cancelled',false),paired('rootfail-held','failed',false),
 {...job('normal01-held','succeeded',7904,[output('normal-A.png'),output('normal-B.png')]),outputRoot:'/captured/normal'},
 job('oldrootless-held','succeeded',7904,[output('legacy.png')])];
for(const held of f.jobs)if(['cancelled','failed'].includes(held.state))held.outputs=held.outputs.filter(o=>o.role==='original');
"""
IPC = r"""
const originalInvoke=window.__TAURI_INTERNALS__.invoke;
const media=path=>'data:image/svg+xml,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="768" height="512"><rect width="768" height="512" fill="'+(path.includes('baseline')?'#175c70':'#98492b')+'"/><circle cx="384" cy="256" r="130" fill="'+(path.includes('baseline')?'#4cd8d0':'#ffd078')+'"/><text x="32" y="470" fill="white" font-size="24">'+path+'</text></svg>');
window.__TAURI_INTERNALS__.convertFileSrc=path=>{f.paths.push(path);return media(path);};
window.__TAURI_INTERNALS__.invoke=async(command,args)=>{
 if(!['native_job_output','native_job_export'].includes(command))return originalInvoke(command,args);
 f.calls.push({command,args:clone(args??{})});
 const keys=Object.keys(args??{}).sort().join(',');
 if(keys!==(command==='native_job_output'?'index,jobId':'index,jobId,title'))return fail('unexpected output argument keys '+keys);
 const held=f.jobs.find(j=>j.id===args.jobId);
 if(!held||held.outputRoot||!['succeeded','failed','cancelled'].includes(held.state)||!Number.isInteger(args.index)||!held.outputs[args.index])return fail('invalid terminal output owner/index');
 if(command==='native_job_export'){
  if(typeof args.title!=='string'||!args.title.trim())return fail('missing export title');
  return {path:'/synthetic/export/'+held.outputs[args.index].filename,sha256:'0'.repeat(64),size:70};
 }
 return {jobId:args.jobId,index:args.index,mimeType:'image/png',width:1,height:1,sha256:'0'.repeat(64),bytes:Array.from(atob(png.split(',')[1]),c=>c.charCodeAt(0))};
};
"""


def fixture():
    assert collection.FIXTURE.count('const snapshot=') == 1
    return collection.FIXTURE.replace('const snapshot=', JOBS+'\nconst snapshot=') + IPC


def pick(page, identifier, index=0):
    queue = page.locator(f'[data-job-id="{identifier}"] .wb-queue-select')
    queue.nth(index).click()
    expect(queue.nth(index)).to_have_attribute('aria-current', 'true')


FINDINGS = []
ARTIFACTS = None


def role_click(page, role):
    button = page.locator(f'[data-output-role="{role}"]')
    try:
        button.click(timeout=1500)
    except Exception as failure:
        if 'intercepts pointer events' not in str(failure):
            raise
        issue = f'Role {role} pointer click intercepted by overlapping preview tools at {page.viewport_size}'
        if issue not in FINDINGS:
            FINDINGS.append(issue)
            assert ARTIFACTS is not None
            page.screenshot(path=str(ARTIFACTS/f'pointer-overlap-{page.viewport_size["width"]}.png'), full_page=True)
        # Continue functional acceptance through genuine keyboard activation; report pointer defect.
        button.focus()
        button.press('Enter')


def loaded(page):
    page.wait_for_function("Array.from(document.querySelectorAll('.wb-shown img')).length>0 && Array.from(document.querySelectorAll('.wb-shown img')).every(i=>i.complete&&i.naturalWidth>0)")


def check_geometry(page, width, locale):
    geometry = page.evaluate("""()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,
      controls:Array.from(document.querySelectorAll('[data-output-role], [data-edit-compare]')).map(el=>{
       const r=el.getBoundingClientRect();return {text:el.textContent,w:r.width,h:r.height,x:r.x,right:r.right,scroll:el.scrollWidth,client:el.clientWidth};
      })})""")
    assert geometry['scroll'] <= width, (width, locale, geometry)
    assert geometry['controls'], (width, locale)
    for control in geometry['controls']:
        assert control['w'] > 15 and control['h'] > 15 and control['x'] >= 0 and control['right'] <= width, (width, locale, control)
        assert control['scroll'] <= control['client'] + 1, (width, locale, control)
        assert control['text'].strip() and 'studio.jobs.' not in control['text'], control


def verify(browser, url, screenshots):
    global ARTIFACTS
    ARTIFACTS = screenshots
    original_fixture = collection.FIXTURE
    collection.FIXTURE = fixture()
    try:
        context, page, errors = collection.open_studio(browser, url, False)
    finally:
        collection.FIXTURE = original_fixture
    passed = []
    try:
        pick(page, 'paired01-held')
        expect(page.locator('[data-output-role="processed"]')).to_have_attribute('aria-pressed', 'true')
        expect(page.locator('.wb-shown img')).to_have_attribute('alt', 'paired01-held-final.png')
        processed = page.locator('.wb-shown img').get_attribute('src')
        role_click(page, 'original')
        expect(page.locator('[data-output-role="original"]')).to_have_attribute('aria-pressed', 'true')
        expect(page.locator('.wb-shown img')).to_have_attribute('alt', 'paired01-held-baseline.png')
        baseline = page.locator('.wb-shown img').get_attribute('src')
        assert baseline != processed
        pick(page, 'paired01-held', 1)
        expect(page.locator('[data-output-role="original"]')).to_have_attribute('aria-pressed', 'true')
        page.locator('[data-edit-compare]').click()
        expect(page.locator('.wb-compare img')).to_have_count(2)
        assert page.locator('.wb-compare > img').get_attribute('src') == baseline
        assert page.locator('.wb-compare-top img').get_attribute('src') == processed
        page.locator('.wb-compare input').fill('27')
        expect(page.locator('.wb-compare')).to_have_attribute('style', '--divide: 27%;')
        loaded(page)
        passed.append('paired success: final-first, role/queue selection, compare uses retained baseline and final')

        for identifier, state in [('cancel02-held', 'cancelled'), ('failed03-held', 'failed')]:
            pick(page, identifier)
            expect(page.locator('.wb-job-line [data-state]')).to_have_attribute('data-state', state)
            expect(page.locator('[data-output-role="original"]')).to_have_attribute('aria-pressed', 'true')
            expect(page.locator('[data-output-role="processed"]')).to_have_count(0)
            expect(page.locator('.wb-shown img')).to_have_attribute('alt', identifier+'-baseline.png')
            expect(page.locator('[data-edit-compare]')).to_have_count(0)
            loaded(page)
        passed.append('failed/cancelled retain accessible original and their non-success status')

        for identifier, state, indices in [('rootless-held', 'succeeded', [0, 1]), ('rootcancel-held', 'cancelled', [0]), ('rootfail-held', 'failed', [0]), ('oldrootless-held', 'succeeded', [0])]:
            for index in indices:
                pick(page, identifier, index)
                expect(page.locator('[data-job-output-preview]')).to_be_visible()
                loaded(page)
                expect(page.locator('.wb-job-line [data-state]')).to_have_attribute('data-state', state)
                expect(page.locator('[data-edit-compare]')).to_have_count(0)
                page.locator('[data-job-output-export]').click()
                expect(page.locator('[data-job-output-exported]')).to_contain_text('/synthetic/export/')
                calls = page.evaluate("__collection.calls.filter(c=>['native_job_output','native_job_export'].includes(c.command)).slice(-2)")
                assert [c['command'] for c in calls] == ['native_job_output', 'native_job_export'], calls
                assert all(c['args']['jobId'] == identifier and c['args']['index'] == index for c in calls), calls
                assert page.evaluate('(id)=>__collection.jobs.find(j=>j.id===id).state', identifier) == state
        passed.append('rootless preview/export exact native owner/index for both roles, failed, cancelled, legacy')

        pick(page, 'normal01-held')
        expect(page.locator('[data-output-role]')).to_have_count(0)
        expect(page.locator('.wb-shown img')).to_have_count(2)
        assert page.locator('.wb-shown img').evaluate_all('(imgs)=>imgs.map(i=>i.alt)') == ['normal-A.png', 'normal-B.png']
        pick(page, 'normal01-held', 1)
        expect(page.locator('.wb-shown img')).to_have_count(2)
        passed.append('legacy no-role rooted multi-output and rootless jobs unchanged')

        for width in (1440, 320):
            page.set_viewport_size({'width': width, 'height': 960})
            for locale in collection.LOCALES:
                collection.set_locale(page, locale, '[data-node="studio"]')
                pick(page, 'paired01-held')
                expect(page.locator('[data-output-role="processed"]')).to_have_attribute('aria-pressed', 'true')
                role_click(page, 'original')
                page.locator('[data-edit-compare]').click()
                loaded(page)
                check_geometry(page, width, locale)
                labels = page.locator('[data-output-role]').all_text_contents()
                translations = json.loads((Path(__file__).resolve().parents[1]/'src/features/studio/locales'/f'{locale}.json').read_text())
                assert labels == [translations['studio.jobs.role.processed'], translations['studio.jobs.role.original']], (locale, labels)
                occluded = page.locator('[data-output-role]').evaluate_all("""buttons=>buttons.filter(button=>{
                    const r=button.getBoundingClientRect();const hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
                    return !hit||!button.contains(hit);
                }).map(button=>button.textContent)""")
                if occluded:
                    FINDINGS.append(f'{width}px {locale}: role button centers occluded: {occluded}')
                shot = screenshots / f'paired-{width}-{locale}.png'
                page.screenshot(path=str(shot), full_page=True)
                collection.assert_clean(page, errors)
                passed.append(f'localized labels and bounds {width}px {locale}')
                print('SCREENSHOT '+str(shot), flush=True)
        collection.assert_clean(page, errors)
        forbidden = page.evaluate("__collection.calls.filter(c=>/run|stop|submit|prepare|interrupt|cancel/.test(c.command))")
        assert not forbidden, forbidden
        result = {'status': 'FAIL' if FINDINGS else 'PASS', 'functional_checks_passed': len(passed), 'checks': passed, 'product_findings': FINDINGS, 'kind': 'actual Chromium; synthetic media; strict simulated Tauri IPC; export is simulated, not filesystem acceptance', 'screenshots': str(screenshots)}
        (screenshots/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        print(json.dumps(result, indent=2), flush=True)
    finally:
        context.close()


def main(*, serve=True):
    root = Path(__file__).resolve().parents[1]
    scratch = Path.home()/'.hermes/cache/scratch'
    scratch.mkdir(parents=True, exist_ok=True)
    screenshots = Path(tempfile.mkdtemp(prefix='deas-postprocessing-', dir=scratch))
    def run(url):
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch()
            try:
                print('CHROMIUM '+browser.version, flush=True)
                verify(browser, url, screenshots)
            finally:
                browser.close()
    # The offline editor needs only file://, not a running app or Vite child.
    if not serve:
        run(root.as_uri())
        return
    with socket.socket() as reserve:
        reserve.bind(('127.0.0.1', 0))
        port = reserve.getsockname()[1]
    url = f'http://127.0.0.1:{port}'
    with (screenshots/'vite.log').open('w') as log:
        server = subprocess.Popen(['node', str(root/'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', str(port), '--strictPort'], cwd=root, stdout=log, stderr=subprocess.STDOUT)
        try:
            for _ in range(100):
                if server.poll() is not None:
                    raise RuntimeError(f'isolated Vite exited; see {screenshots}/vite.log')
                try:
                    with urlopen(url, timeout=0.2) as response:
                        if response.status == 200:
                            break
                except OSError:
                    time.sleep(0.1)
            else:
                raise RuntimeError('isolated Vite startup timeout')
            print(f'ISOLATED_VITE {url} pid={server.pid}; artifacts={screenshots}', flush=True)
            run(url)
        finally:
            server.terminate()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait(timeout=5)
            print(f'CLEANUP isolated Vite pid={server.pid} exit={server.returncode}', flush=True)


if __name__ == '__main__':
    main()
    raise SystemExit(1 if FINDINGS else 0)
