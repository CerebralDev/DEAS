"""Integrated metadata editor acceptance; strict simulated IPC, no models or services.

Run: uv run --no-project --with playwright python tools/verify_model_metadata.py
Reuses the isolated Vite runner; screenshots/results remain in Hermes scratch.
"""
import json
from playwright.sync_api import expect
import verify_postprocessing as runner
import verify_workbench_collection as collection
import verify_workspace_layout as layout

FIXTURE = layout.FIXTURE + r'''
const metadataInvoke=window.__TAURI_INTERNALS__.invoke;
const unknown={category:'diffusion_models',name:'nested/unknown.safetensors',path:'/fixture/models/diffusion_models/nested/unknown.safetensors',size:42,info:null};
const invalid={...unknown,category:'checkpoints',name:'nested/retired.safetensors',path:'/fixture/models/checkpoints/nested/retired.safetensors',detectedFamily:'sdxl',metadataError:'Unknown model family',info:{sha256:'a'.repeat(64),size:42,fetchedAt:1,civitai:{modelId:2,versionId:1,modelName:'Fixture',versionName:'v1'}}};
const ace={...unknown,name:'nested/acestep_v1.5_turbo.safetensors',path:'/fixture/models/diffusion_models/nested/acestep_v1.5_turbo.safetensors',family:'ace-step-1-5-turbo',detectedFamily:'ace-step-1-5-turbo',familyEvidence:'filename'};
f.models=[unknown,invalid,ace];
window.__TAURI_INTERNALS__.invoke=async(c,a)=>{
 if(c==='native_download_catalog'){f.calls.push({command:c,args:clone(a??{})});return {version:1,providers:[],entries:[]};}
 if(c==='native_model_scan_status'){f.calls.push({command:c,args:clone(a??{})});return {state:'idle',completed:0,total:0,remaining:0,failed:0};}
 if(c==='native_settings_read'){f.calls.push({command:c,args:clone(a??{})});return {models:'/fixture/models'};}
 if(c==='native_models_scan'){f.calls.push({command:c,args:clone(a??{})});return clone(f.models);}
 if(c==='native_model_family_write'){
  f.calls.push({command:c,args:clone(a??{})});
  if(Object.keys(a).sort().join(',')!=='family,path')return fail('unexpected metadata payload');
  const old=f.models.find(m=>m.path===a.path);if(!old)return fail('foreign metadata path');
  const updated={...old};delete updated.metadataError;delete updated.familyOverride;
  if(a.family===null)updated.family=old.detectedFamily;
  else {if(!['anima','ace-step-1-5-sft'].includes(a.family))return fail('unexpected family');updated.family=a.family;updated.familyOverride=a.family;}
  f.models=f.models.map(m=>m.path===a.path?updated:m);return clone(updated);
 }
 return metadataInvoke(c,a);
};
'''


def verify(browser, url, shots):
    original = collection.FIXTURE
    collection.FIXTURE += FIXTURE
    try:
        context, page, errors = collection.open_studio(browser, url, False)
    finally:
        collection.FIXTURE = original
    checks = []
    try:
        page.locator('[data-dock-tab="models"]').click()
        tile = page.get_by_role('button', name='nested/unknown.safetensors', exact=True)
        expect(tile).to_have_attribute('aria-disabled', 'true')
        tile.focus()
        tile.press('Shift+F10')
        page.get_by_role('menuitem', name='Edit meta…').click()
        dialog = page.get_by_role('dialog')
        expect(dialog).to_be_visible()
        page.get_by_label('Model family', exact=True).select_option('anima')
        page.get_by_role('button', name='Save', exact=True).click()
        expect(dialog).not_to_be_visible()
        expect(tile).to_have_attribute('aria-disabled', 'false')
        expect(tile).to_contain_text('User-declared family')
        checks.append('keyboard edit enables unknown model without submitting')
        # Library and Studio use the same metadata action and exact-path event updates.
        page.get_by_role('tab', name='Library', exact=True).click()
        page.locator('[data-library-tab="installed"]').click()
        library = page.get_by_role('button', name='nested/retired.safetensors', exact=True)
        library.click(button='right')
        page.get_by_role('menuitem', name='Edit meta…').click()
        expect(dialog.get_by_role('alert')).to_be_visible()
        page.get_by_role('button', name='Reset to detection').click()
        expect(dialog).not_to_be_visible()
        page.locator('[data-node="studio"]').click()
        expect(page.get_by_role('button', name='nested/retired.safetensors', exact=True)).to_have_attribute('aria-disabled', 'false')
        checks.append('Library reset repairs invalid metadata and updates mounted Studio')
        ace_tile = page.get_by_role('button', name='nested/acestep_v1.5_turbo.safetensors', exact=True)
        ace_tile.press('Shift+F10')
        page.get_by_role('menuitem', name='Edit meta…').click()
        expect(dialog.locator('[data-model-family-evidence]')).to_have_attribute('data-model-family-evidence', 'filename')
        expect(dialog.locator('[data-model-family-evidence]')).to_contain_text('not confirmed by the header')
        page.get_by_label('Model family', exact=True).select_option('ace-step-1-5-sft')
        page.get_by_role('button', name='Save', exact=True).click()
        expect(dialog).not_to_be_visible()
        ace_tile.press('Shift+F10')
        page.get_by_role('menuitem', name='Edit meta…').click()
        expect(page.get_by_label('Model family', exact=True)).to_have_value('ace-step-1-5-sft')
        expect(dialog.locator('[data-model-family-evidence]')).to_have_attribute('data-model-family-evidence', 'filename')
        page.get_by_role('button', name='Reset to detection').click()
        expect(dialog).not_to_be_visible()
        checks.append('ACE filename evidence stays distinct from declared variant and reset')
        for locale in collection.LOCALES:
            collection.set_locale(page, locale, '[data-node="studio"]')
            for width in (1440, 768, 320):
                page.set_viewport_size({'width': width, 'height': 900})
                ace_tile.press('Shift+F10')
                page.get_by_role('menuitem').click()
                expect(dialog).to_be_visible()
                bounds = dialog.evaluate('e=>{const r=e.getBoundingClientRect();return {x:r.x,right:r.right,bottom:r.bottom,scroll:e.scrollWidth,client:e.clientWidth}}')
                assert bounds['x'] >= 0 and bounds['right'] <= width + 1, (locale, width, bounds)
                assert bounds['bottom'] <= 901 and bounds['scroll'] <= bounds['client'] + 1, (locale, width, bounds)
                assert 'modelMeta.' not in dialog.inner_text()
                if locale in ('de', 'en') and width in (1440, 320):
                    page.screenshot(path=str(shots/f'metadata-{locale}-{width}.png'))
                dialog.press('Escape')
                expect(dialog).not_to_be_visible()
                checks.append(f'{locale}:{width}:dialog fits and Escape closes')
        calls = page.evaluate('__collection.calls')
        writes = [c for c in calls if c['command'] == 'native_model_family_write']
        assert [c['args']['family'] for c in writes] == ['anima', None, 'ace-step-1-5-sft', None], writes
        forbidden = ('native_workbench_run', 'native_job_prepare', 'native_job_submit', 'native_model_info', 'native_download_start', 'native_backend_start')
        assert not [c for c in calls if c['command'] in forbidden]
        collection.assert_clean(page, errors)
        result = {'checks': checks, 'writes': writes, 'errors': errors, 'evidence': 'Chromium UI with simulated native IPC; Rust filesystem tests verify actual sidecar preservation separately'}
        (shots/'result.json').write_text(json.dumps(result, indent=2))
        print(json.dumps(result, indent=2), flush=True)
    finally:
        context.close()


if __name__ == '__main__':
    runner.verify = verify
    runner.main()
