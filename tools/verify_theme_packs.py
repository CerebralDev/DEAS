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
    result = []
    root = Path(__file__).resolve().parents[1]/'theme-packs'
    for group in ['classic', 'optional']:
        for file in sorted((root/group/'themes').glob('*/theme.json')):
            data = json.loads(file.read_text(encoding='utf-8-sig'))
            data.pop('schemaVersion')
            data['presentation'] = data.get('presentation')
            data['effects'] = data.get('effects', {'mouse': False, 'wine': False})
            data['assets'] = {key: 'data:image/'+('webp' if name.endswith('.webp') else 'png')+';base64,'+base64.b64encode((file.parent/name).read_bytes()).decode() for key, name in data.get('assets', {}).items()}
            result.append(data)
    assert result, 'No theme pack manifests found'
    return result


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
    data = packs()
    classic = [p for p in data if p['presentation'] is None]
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
'''.replace('__PACKS__', json.dumps(classic))
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
        expect(page.locator('html')).to_have_attribute('data-theme-pack', 'default')
        page.locator('[data-run-positive]').fill('Keep this prompt while changing themes')
        for pack in classic:
            select(pack['id'])
            expect(page.locator('html')).to_have_attribute('data-theme-pack', pack['id'])
            assert page.evaluate("getComputedStyle(document.documentElement).getPropertyValue('--panel').trim()") == pack['colors']['panel']
            return_to_studio()
            expect(page.locator('html')).to_have_attribute('data-theme-pack', pack['id'])
            expect(page.locator('[data-run-positive]')).to_have_value('Keep this prompt while changing themes')
        select('android-white')
        page.evaluate("window.__themePacks=window.__themePacks.filter(p=>p.id!=='android-white')")
        refresh()
        assert page.locator('html').get_attribute('data-theme') is None
        assert page.locator('html').get_attribute('data-theme-pack') is None
        return_to_studio()
        quick()
        expect(page.locator('[data-scheme="android-white"]')).to_have_count(0)
        page.locator('.studio-quick > button').click()
        page.evaluate('values=>window.__themePacks=values', data)
        refresh()
        expect(page.locator('html')).to_have_attribute('data-theme-pack', 'android-white')
        select('french')
        expect(page.locator('[data-mouse-house]')).to_be_visible()
        expect(page.locator('[data-french-wine]')).to_be_visible()
        expect(page.locator('[data-french-wine]')).to_have_attribute('aria-valuenow', '25')
        cursor = page.locator('.studio-quick > button').evaluate('el=>getComputedStyle(el).cursor')
        assert 'data:image/png;base64,' in cursor, cursor
        door = page.locator('.mouse-door')
        for _ in range(4):
            door.click()
        expect(page.locator('[data-cheese-mouse]')).to_be_visible()
        page.keyboard.press('Escape')
        for identifier in ['french', 'paris']:
            select(identifier)
            if identifier == 'paris':
                expect(page.locator('.paris-scene')).to_be_visible()
            for width, height in [(1440, 960), (1024, 768), (390, 844)]:
                page.set_viewport_size({'width': width, 'height': height})
                page.mouse.move(4, 4)
                page.wait_for_timeout(150)
                bounds = page.evaluate('({w:innerWidth,h:innerHeight,sw:document.documentElement.scrollWidth,sh:document.documentElement.scrollHeight})')
                assert bounds['sw'] <= width+1 and bounds['sh'] <= height+2, (identifier, bounds)
                page.screenshot(path=str(shots/(identifier+'-'+str(width)+'.png')))
            page.set_viewport_size({'width': 1440, 'height': 960})
        select('french')
        page.evaluate('values=>window.__themePacks=values', classic)
        refresh()
        expect(page.locator('[data-mouse-house]')).to_have_count(0)
        assert page.evaluate("document.documentElement.style.getPropertyValue('--theme-asset-cheese')") == ''
        page.evaluate('values=>window.__themePacks=values', data)
        refresh()
        expect(page.locator('[data-mouse-house]')).to_be_visible()
        select('sage')
        page.evaluate("window.__themePacks.find(p=>p.id==='sage').colors.ok='#ff0000'")
        refresh()
        assert page.evaluate("document.documentElement.style.getPropertyValue('--ok')") == '#ff0000'
        page.reload(wait_until='networkidle')
        expect(page.locator('html')).to_have_attribute('data-theme-pack', 'sage')
        collection.assert_clean(page, errors)
        forbidden = page.evaluate("__collection.calls.filter(c=>/(_start|_stop|_write|_send|_generate|_run|_submit|_cancel)$/.test(c.command))")
        assert not forbidden, forbidden
        result = {'status': 'PASS', 'kind': 'Chromium with strict simulated IPC and actual pack manifests/assets', 'checks': ['all supplied classic palettes and navigation retention', 'deleted active Android White disappears; Default recovery and restoration', 'legacy IDs and reload', 'prompt retention', 'runtime refresh after palette edit', 'removed pack fallback and asset cleanup', 'restored pack reapplies', 'French PNG cursor', 'mouse door activation and Escape', 'wine progress follows retained native job percentage', 'Paris scene', 'six optional-theme viewports', 'no remote requests, writes or generation'], 'screenshots': str(shots)}
        (shots/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        print(json.dumps(result, indent=2), flush=True)
    finally:
        context.close()


if __name__ == '__main__':
    runner.verify = verify
    runner.main()
