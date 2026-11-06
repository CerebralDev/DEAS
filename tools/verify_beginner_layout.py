"""Beginner/Advanced layout parity; Chromium and strict simulated IPC only.

Run: uv run --no-project --with playwright python tools/verify_beginner_layout.py
Reuses the workspace fixture and owned Vite runner; artifacts stay in Hermes scratch.
"""
import json

from playwright.sync_api import expect
import verify_workspace_layout as workspace

collection = workspace.collection
runner = workspace.runner

STYLE = r"""e=>{
 const s=getComputedStyle(e);
 return Object.fromEntries(['backgroundColor','backgroundImage','color','borderTopColor',
  'borderTopWidth','borderTopStyle','borderBottomColor','borderBottomWidth',
  'borderBottomStyle','borderRadius','boxShadow','fontFamily','fontSize',
  'fontWeight','lineHeight','paddingTop','paddingRight','paddingBottom','paddingLeft',
  'textTransform','height'].map(k=>[k,s[k]]));
}"""


def switch_mode(page, mode, shots=None):
    menu = page.locator('.studio-quick > button')
    if menu.get_attribute('aria-expanded') != 'true':
        menu.click()
    button = page.locator('.experience-switch').get_by_role('switch')
    assert abs(button.bounding_box()['width'] - page.get_by_role('button', name='Reset layout', exact=True).bounding_box()['width']) < 1
    if button.get_attribute('aria-checked') != str(mode == 'advanced').lower():
        button.press('Space')
    expect(button).to_have_attribute('aria-checked', str(mode == 'advanced').lower())
    if shots:
        page.screenshot(path=str(shots/'experience-lightswitch.png'))
    page.keyboard.press('Escape')
    expect(page.locator('[data-run-positive]')).to_be_visible()
    page.evaluate('()=>document.fonts.ready')
    page.mouse.move(0, 0)
    page.evaluate('()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))')


def run_radius(locator, property):
    return locator.evaluate('(e,p)=>getComputedStyle(e)[p]', property)


def snapshot(page, mode, width, mixed, shots):
    label = f'{mode}-{width}-{"active" if mixed else "empty"}'
    expect(page.locator('.beginner-app-logo')).to_have_count(0)
    selected = page.locator('.aio-tabs [aria-selected="true"]')
    expect(selected).to_have_count(1)
    inactive = page.locator('.aio-tabs [aria-selected="false"]').first
    expect(inactive).to_be_visible()
    styles = {'selectedTab': selected.evaluate(STYLE), 'inactiveTab': inactive.evaluate(STYLE)}
    positive = page.locator('[data-run-positive]')
    expect(positive).to_have_value('Retained positive-only layout draft')
    if mode == 'beginner':
        expect(page.locator('[data-run-negative]')).to_be_hidden()
        expect(page.locator('.wb-prompts textarea:visible')).to_have_count(1)
        # Catch all current and future prompt extras, not just a toolbar class name.
        expect(page.locator('.wb-prompts button:visible, .wb-prompts [role="toolbar"]:visible')).to_have_count(0)
        expect(page.locator('[data-prompt-count]:visible')).to_have_count(0)
        expect(page.locator('#studio-dock')).to_be_hidden()
        assert run_radius(page.locator('[data-run]'), 'borderTopLeftRadius') == '10px'
        edge = page.locator('[data-run-stop]') if mixed else page.locator('[data-run]')
        assert run_radius(edge, 'borderTopRightRadius') == '10px', label
    else:
        expect(page.locator('[data-run-negative]')).to_be_visible()
        expect(page.locator('#studio-dock')).to_be_visible()
    queue = page.locator('.wb-queue')
    expect(queue).to_be_visible()
    expect(page.locator(collection.ROWS)).to_have_count(5 if mixed else 0)
    run = page.locator('[data-run]')
    expect(run).to_be_visible()
    assert run.evaluate("e=>e.closest('.wb-queue')!==null"), label
    styles['run'] = run.evaluate(STYLE)
    styles['run'].pop('borderRadius')
    styles['runDisabled'] = run.is_disabled()
    styles['runType'] = run.get_attribute('type')
    menu = page.locator('#project-actions-slot summary')
    assert menu.is_visible() == (mode == 'advanced'), label
    if menu.is_visible():
        menu.evaluate(STYLE)
        menu.click()
        page.locator('#project-actions-slot button').evaluate_all(
            "els=>els.map(e=>({text:e.textContent,disabled:e.disabled,action:[...e.attributes].find(a=>a.name.startsWith('data-project-'))?.name}))")
        menu.press('Escape')
    stop = page.locator('[data-run-stop]')
    if mixed:
        expect(stop).to_be_visible()
        expect(stop).to_be_enabled()
        styles['stop'] = stop.evaluate(STYLE)
        styles['stop'].pop('borderRadius')
        styles['stopLabel'] = stop.get_attribute('aria-label')
        styles['stopTitle'] = stop.get_attribute('title')
        if mode == 'beginner':
            run_box,stop_box=run.bounding_box(),stop.bounding_box()
            assert abs(run_box['x']+run_box['width']-stop_box['x']) < 1, label
            assert abs(run_box['height']-stop_box['height']) < 1, label
    else:
        expect(stop).to_have_count(0)
    geometry = page.evaluate(r"""()=>{
      const rect=s=>{const r=document.querySelector(s).getBoundingClientRect();
       return {x:r.x,y:r.y,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
      return {width:innerWidth,scroll:document.documentElement.scrollWidth,
       queue:rect('.wb-queue'),center:rect('.wb-center'),prompt:rect('[data-run-positive]')};
    }""")
    assert geometry['scroll'] <= width + 1, (label, geometry)
    if width >= 1024:
        assert geometry['queue']['x'] >= geometry['center']['right'] - 1, (label, 'queue not right-hand', geometry)
        assert geometry['queue']['width'] > 100, (label, geometry)
    if mode == 'beginner' and width >= 1024:
        expect(page.locator('.beginner-studio-heading, .beginner-local-badge')).to_have_count(0)
        for selector, key in (('.wb-column-handle[aria-controls="studio-settings"]', 'ArrowRight'), ('[data-beginner-prompt-resize]', 'ArrowUp')):
            handle = page.locator(selector)
            expect(handle).to_be_visible()
            before = handle.get_attribute('aria-valuenow')
            handle.press(key)
            assert handle.get_attribute('aria-valuenow') != before, (label, selector)
        height=positive.bounding_box()['height']
        handle=page.locator('[data-beginner-prompt-resize]')
        box=handle.bounding_box()
        before=int(handle.get_attribute('aria-valuenow'))
        page.mouse.move(box['x']+20,box['y']+4)
        page.mouse.down()
        page.mouse.move(box['x']+20,box['y']-20,steps=4)
        page.mouse.up()
        assert int(handle.get_attribute('aria-valuenow')) == before+24, label
        assert positive.bounding_box()['height'] > height+20, label
        preview=page.locator('.wb-stage').bounding_box()
        divider=page.locator('.beginner-prompt-divider').bounding_box()
        prompts=page.locator('#studio-prompts').bounding_box()
        assert preview['y']+preview['height'] <= divider['y']+1, (label,preview,divider)
        assert divider['y']+divider['height'] <= prompts['y']+1, (label,divider,prompts)
        for pane in ('settings', 'prompt'):
            toggle = page.locator(f'[data-pane-toggle="{pane}"]')
            toggle.click()
            expect(toggle).to_have_attribute('aria-expanded', 'false')
            toggle.click()
            expect(toggle).to_have_attribute('aria-expanded', 'true')
    positive.scroll_into_view_if_needed()
    assert positive.bounding_box()['width'] > 50, (label, geometry)
    assert positive.bounding_box()['height'] >= 60, (label, positive.bounding_box())
    prompt_shot = shots/f'{label}.png'
    page.screenshot(path=str(prompt_shot))
    run.scroll_into_view_if_needed()
    run_box = run.bounding_box()
    assert run_box and 0 <= run_box['y'] and run_box['y'] + run_box['height'] <= page.viewport_size['height'] + 1, (label, run_box)
    queue_shot = shots/f'{label}-queue.png'
    page.screenshot(path=str(queue_shot))
    return {'label': label, 'styles': styles, 'geometry': geometry,
            'screenshots': [str(prompt_shot), str(queue_shot)]}


def verify(browser, url, shots):
    original = collection.FIXTURE
    collection.FIXTURE += workspace.FIXTURE + r'''
localStorage.setItem('deas.pref.interface.experience', JSON.stringify('advanced'));
'''
    checks = []
    findings = []
    try:
        for mixed in (False, True):
            context, page, errors = collection.open_studio(browser, url, mixed)
            try:
                advanced_backend = page.locator('[data-node="settings"]').evaluate(STYLE)
                page.locator('[data-run-positive]').fill('Retained positive-only layout draft')
                switch_mode(page, 'beginner', shots)
                backend = page.locator('.beginner-backend')
                assert backend.inner_text() == 'Backend'
                assert backend.evaluate(STYLE) == advanced_backend
                backend.click()
                expect(backend).to_have_attribute('aria-selected', 'true')
                expect(page.locator('[data-beginner-tab][aria-selected="true"]')).to_have_count(0)
                page.locator('[data-beginner-tab="generate"]').click()
                expect(backend).to_have_attribute('aria-selected', 'false')
                for width, height in ((1440, 960), (390, 844)):
                    page.set_viewport_size({'width': width, 'height': height})
                    pair = []
                    for mode in ('advanced', 'beginner'):
                        switch_mode(page, mode)
                        pair.append(snapshot(page, mode, width, mixed, shots))
                    for key in pair[0]['styles'].keys() | pair[1]['styles'].keys():
                        advanced = pair[0]['styles'].get(key)
                        beginner = pair[1]['styles'].get(key)
                        if advanced != beginner:
                            findings.append({'width': width, 'activeQueue': mixed, 'feature': key,
                                             'advanced': advanced, 'beginner': beginner})
                    checks.extend(pair)
                switch_mode(page, 'advanced')
                expect(page.locator('[data-run-positive]')).to_have_value('Retained positive-only layout draft')
                if mixed:
                    collection.row(page, 'unknown3-held').click()
                    expect(collection.row(page, 'unknown3-held')).to_have_attribute('aria-current', 'true')
                    switch_mode(page, 'beginner')
                    expect(collection.row(page, 'unknown3-held')).to_have_attribute('aria-current', 'true')
                collection.assert_clean(page, errors)
                forbidden = page.evaluate("__collection.calls.filter(c=>/(_start|_stop|_write|_send|_generate|_run|_submit|_cancel)$/.test(c.command))")
                assert not forbidden, forbidden
            except Exception:
                page.screenshot(path=str(shots/'failure.png'))
                raise
            finally:
                context.close()
    finally:
        collection.FIXTURE = original
    result = {'status': 'FAIL' if findings else 'PASS', 'casesChecked': len(checks),
              'parityFindings': findings, 'checks': checks,
              'kind': 'actual Chromium; strict simulated IPC only; no native/backend/GPU execution',
              'screenshots': str(shots)}
    (shots/'beginner-layout-result.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result, indent=2))
    assert not findings, f'{len(findings)} parity findings; see {shots}/beginner-layout-result.json'


if __name__ == '__main__':
    runner.verify = verify
    runner.main()
