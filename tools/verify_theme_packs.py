"""Actual theme packs rendered through strict synthetic IPC; no real backend or writes."""
import base64
import json
from pathlib import Path
from urllib.parse import urlparse
from playwright.sync_api import expect
import verify_postprocessing as runner
import verify_workbench_collection as collection
import verify_workspace_layout as layout


def packs():
    file = Path(__file__).resolve().parents[1]/'theme-packs/new-theme/theme.json'
    data = json.loads(file.read_text(encoding='utf-8-sig'))
    data.pop('schemaVersion')
    data.setdefault('colors', {})
    data.setdefault('assets', {})
    return [data]


def verify(browser, url, shots):
    context = browser.new_context(viewport={'width': 1440, 'height': 960}, reduced_motion='reduce')
    page = context.new_page()
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    origin = urlparse(url).netloc
    def route(r):
        if urlparse(r.request.url).netloc != origin:
            errors.append('Unexpected remote resource')
            r.abort()
        else:
            r.continue_()
    page.route('http**/*', route)
    packs_data = packs()
    fixture = collection.FIXTURE.replace('__MIXED__', 'false').replace('const snapshot=', "f.jobs=[{...job('theme-running','running',7902),progress:{value:25,max:100}}];const snapshot=")
    script = fixture+layout.FIXTURE+'''
localStorage.setItem('deas.locale.v1','en');
window.__themePacks=__PACKS__;
const themeInvoke=window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke=async(c,a)=>{
 if(c==='native_theme_packs'){f.calls.push({command:c,args:clone(a??{})});return {directory:'/fixture/env/themes',packs:clone(window.__themePacks),warnings:[]};}
 if(c==='native_wildcard_library')return JSON.stringify({categories:[]});
 return themeInvoke(c,a);
};
'''.replace('__PACKS__', json.dumps(packs_data))
    page.add_init_script(script)
    def quick():
        if not page.locator('#studio-quick-actions').count():
            page.locator('.studio-quick > button').click()
    def select(identifier):
        quick()
        page.locator('[data-scheme="'+identifier+'"]').click()
        page.locator('.studio-quick > button').click()
    def refresh():
        quick()
        page.locator('[data-theme-action="refresh"]').click()
        expect(page.locator('[data-theme-action="refresh"]')).to_be_enabled()
        page.locator('.studio-quick > button').click()
    def return_to_studio():
        page.locator('[data-node="settings"]').click()
        page.locator('[data-studio-tab="generate"]').click()
    try:
        page.goto(url, wait_until='networkidle')
        page.locator('[data-studio-tab="generate"]').click()
        assert page.locator('html').get_attribute('data-theme') in (None, 'default')  # built-in fallback
        assert page.locator('html').get_attribute('data-theme-pack') is None
        page.locator('[data-run-positive]').fill('Keep this prompt while changing themes')
        select('new-theme')
        expect(page.locator('html')).to_have_attribute('data-theme-pack', 'new-theme')
        return_to_studio()
        expect(page.locator('html')).to_have_attribute('data-theme-pack', 'new-theme')
        expect(page.locator('[data-run-positive]')).to_have_value('Keep this prompt while changing themes')

        page.evaluate('window.__themePacks=[]')
        refresh()
        select('android-white')
        assert page.locator('html').get_attribute('data-theme') == 'android-white'  # built-in remains available
        assert page.locator('html').get_attribute('data-theme-pack') is None
        return_to_studio()
        quick()
        expect(page.locator('[data-scheme="android-white"]')).to_have_count(1)
        page.locator('.studio-quick > button').click()
        page.evaluate('values=>window.__themePacks=values', packs_data)
        refresh()
        select('new-theme')

        page.evaluate("window.__themePacks.find(p=>p.id==='new-theme').colors.ok='#ff0000'")
        refresh()
        assert page.evaluate("document.documentElement.style.getPropertyValue('--ok')") == '#ff0000'
        page.reload(wait_until='networkidle')
        expect(page.locator('html')).to_have_attribute('data-theme-pack', 'new-theme')
        collection.assert_clean(page, errors)
        forbidden = page.evaluate("__collection.calls.filter(c=>/(_start|_stop|_write|_send|_generate|_run|_submit|_cancel)$/.test(c.command))")
        assert not forbidden, forbidden
        result = {'status': 'PASS', 'kind': 'Chromium with strict simulated IPC and actual starter manifest', 'checks': ['starter pack applies and navigation is retained', 'built-ins remain available without pack duplication', 'unique starter pack reload', 'prompt retention', 'runtime refresh after palette edit', 'no remote requests, writes or generation'], 'screenshots': str(shots)}
        (shots/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        print(json.dumps(result, indent=2), flush=True)
    finally:
        context.close()


if __name__ == '__main__':
    runner.verify = verify
    runner.main()
