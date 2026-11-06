"""ACE Text to Music authoring acceptance; strict simulated IPC, no model execution.

Run: CHOKIDAR_USEPOLLING=1 uv run --no-project --with playwright==1.64.1 python tools/verify_ace_music.py
Uses an isolated Vite child. Native graph/schema acceptance is a separate gate.
"""
import json
from playwright.sync_api import expect
import verify_postprocessing as runner
import verify_workbench_collection as collection
import verify_workspace_layout as layout

FIXTURE = layout.FIXTURE + r'''
localStorage.setItem('deas.pref.studio.run.pipeline', JSON.stringify('t2m'));
const aceInvoke=window.__TAURI_INTERNALS__.invoke;
const aceModel=(name,family,category='diffusion_models')=>({name,family,detectedFamily:family,familyEvidence:'filename',category,path:'/fixture/models/'+category+'/'+name,size:42,info:null});
f.models=[
 aceModel('ace_step_v1_3.5b.safetensors','ace-step-1-0','checkpoints'),
 aceModel('ace_step_1.5_turbo_aio.safetensors','ace-step-1-5-turbo','checkpoints'),
 aceModel('acestep_v1.5_turbo.safetensors','ace-step-1-5-turbo'),
 aceModel('acestep_v1.5_base.safetensors','ace-step-1-5-base','unet'),
 aceModel('acestep_v1.5_sft.safetensors','ace-step-1-5-sft'),
 aceModel('acestep_v1.5_xl_base_bf16.safetensors','ace-step-1-5-xl-base'),
 aceModel('acestep_v1.5_xl_sft_bf16.safetensors','ace-step-1-5-xl-sft'),
 aceModel('acestep_v1.5_xl_turbo_bf16.safetensors','ace-step-1-5-xl-turbo'),
 aceModel('renamed_ace.safetensors','ace-step-1-5'),
 ...['qwen_0.6b_ace15.safetensors','qwen_1.7b_ace15.safetensors','qwen_4b_ace15.safetensors'].map(name=>({name,category:'text_encoders',path:'/fixture/models/text_encoders/'+name,size:42,info:null})),
 {name:'ace_1.5_vae.safetensors',category:'vae',path:'/fixture/models/vae/ace_1.5_vae.safetensors',size:42,info:null}
];
const aceNodes=['CheckpointLoaderSimple','KSampler','UNETLoader','VAELoader','DualCLIPLoader','TextEncodeAceStepAudio','TextEncodeAceStepAudio1.5','ConditioningZeroOut','EmptyAceStepLatentAudio','EmptyAceStep1.5LatentAudio','ModelSamplingSD3','ModelSamplingAuraFlow','LatentOperationTonemapReinhard','LatentApplyOperationCFG','VAEDecodeAudio','SaveAudioAdvanced'];
f.aceSubmissions=[];
window.__TAURI_INTERNALS__.invoke=async(c,a)=>{
 if(c==='native_models_scan'){f.calls.push({command:c,args:clone(a??{})});return clone(f.models);}
 if(c==='native_settings_read'){f.calls.push({command:c,args:clone(a??{})});return {models:'/fixture/models'};}
 if(c==='native_recipe_prerequisites'){
  f.calls.push({command:c,args:clone(a??{})});
  return {baseUrl:a.baseUrl.replace(/\/$/,'')+'/',recipeId:'checkpoint-text-to-image-diagnostic',version:1,generationReady:false,missingNodes:[],allowlist:{nodes:aceNodes,checkpoints:f.models.filter(m=>m.category==='checkpoints').map(m=>m.name),diffusionModels:f.models.filter(m=>['diffusion_models','unet'].includes(m.category)).map(m=>m.name),textEncoders:['qwen_0.6b_ace15.safetensors','qwen_1.7b_ace15.safetensors','qwen_4b_ace15.safetensors'],dualClipTypes:['ace'],dualTextEncoders:['qwen_0.6b_ace15.safetensors','qwen_1.7b_ace15.safetensors','qwen_4b_ace15.safetensors'],vaes:['ace_1.5_vae.safetensors'],loras:[],samplers:['euler'],schedulers:['simple']}};
 }
 if(c==='native_workbench_run'){
  f.calls.push({command:c,args:clone(a??{})});
  if(a.confirmed!==true||a.admissionEpoch!==0||a.intent?.recipeId!=='music-generation'||a.intent.mode!=='t2m'||!a.intent.family?.startsWith('ace-step-')||!a.intent.style?.trim()||!a.intent.ace||a.intent.generation||a.intent.abc)return fail('invalid ACE authoring submission');
  f.aceSubmissions.push(clone(a.intent));
  const held=job('acerun-'+f.aceSubmissions.length,'queued',7801);
  held.baseUrl=a.baseUrl+'/';held.intent=freeze(clone(a.intent));f.jobs.push(held);return clone(held);
 }
 return aceInvoke(c,a);
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
        picker = page.locator('[data-model-picker]')
        expect(picker.locator('option[value="acestep_v1.5_turbo.safetensors"]')).to_have_count(1)
        expect(picker.locator('option[value="renamed_ace.safetensors"]')).to_have_count(0)
        style = page.locator('[data-music-prompt="style"]')
        lyrics = page.locator('[data-music-prompt="lyrics"]')
        style.fill('Ambient electronic music, soft piano, no vocals')
        lyrics.fill('')
        cases = [
            ('ace_step_v1_3.5b.safetensors', 'ace-step-1-0', 50, 5, 5, False),
            ('ace_step_1.5_turbo_aio.safetensors', 'ace-step-1-5-turbo', 8, 1, 3, False),
            ('acestep_v1.5_turbo.safetensors', 'ace-step-1-5-turbo', 8, 1, 3, True),
            ('acestep_v1.5_base.safetensors', 'ace-step-1-5-base', 50, 7, 1, True),
            ('acestep_v1.5_sft.safetensors', 'ace-step-1-5-sft', 50, 7, 1, True),
            ('acestep_v1.5_xl_base_bf16.safetensors', 'ace-step-1-5-xl-base', 50, 6, 3, True),
            ('acestep_v1.5_xl_sft_bf16.safetensors', 'ace-step-1-5-xl-sft', 50, 7, 3, True),
            ('acestep_v1.5_xl_turbo_bf16.safetensors', 'ace-step-1-5-xl-turbo', 8, 1, 3, True),
        ]
        for name, family, steps, cfg, shift, split in cases:
            picker.select_option(name)
            expect(page.locator('[data-num="steps"]')).to_have_value(str(steps))
            expect(page.locator('[data-num="cfg"]')).to_have_value(str(cfg))
            expect(page.locator('[data-ace-setting="shift"]')).to_have_value(str(shift))
            expect(style).to_have_value('Ambient electronic music, soft piano, no vocals')
            expect(lyrics).to_have_value('')
            expect(page.locator('[data-music-planning]')).to_have_count(0)
            expect(page.locator('.wb-prompts .forge-anchor')).to_have_count(0)
            run = page.locator('[data-run]')
            expect(run).to_be_enabled()
            run.click()
            page.wait_for_function('(n)=>__collection.aceSubmissions.length===n', arg=len(checks)+1)
            held = page.evaluate('__collection.aceSubmissions.at(-1)')
            assert held['family'] == family and held['steps'] == steps and held['cfg'] == cfg, held
            assert held['ace']['shift'] == shift and bool(held.get('diffusion')) == split, held
            if split:
                assert held['diffusion']['textEncoder'] == 'qwen_0.6b_ace15.safetensors', held
                assert held['diffusion']['textEncoder2'] in ('qwen_1.7b_ace15.safetensors','qwen_4b_ace15.safetensors'), held
                assert held['diffusion']['vae'] == 'ace_1.5_vae.safetensors', held
            checks.append(name+': defaults/components/immutable submission')
        # User-authored values must survive another model and return, including empty lyrics.
        picker.select_option('acestep_v1.5_turbo.safetensors')
        page.locator('[data-num="steps"]').fill('19')
        page.locator('[data-ace-setting="bpm"]').fill('137')
        picker.select_option('acestep_v1.5_xl_sft_bf16.safetensors')
        picker.select_option('acestep_v1.5_turbo.safetensors')
        expect(page.locator('[data-num="steps"]')).to_have_value('19')
        expect(page.locator('[data-ace-setting="bpm"]')).to_have_value('137')
        expect(lyrics).to_have_value('')
        checks.append('same-model overrides and authored prompts survive model revisit')
        for locale in collection.LOCALES:
            collection.set_locale(page, locale, '[data-node="studio"]')
            for width in (1440, 390):
                page.set_viewport_size({'width':width,'height':960})
                controls = page.locator('[data-ace-setting="bpm"]')
                controls.scroll_into_view_if_needed()
                assert page.evaluate('document.documentElement.scrollWidth <= innerWidth+1')
                assert 'studio.music.ace.' not in page.locator('.wb-side').inner_text()
                if locale in ('en','de'):
                    page.screenshot(path=str(shots/f'ace-{locale}-{width}.png'))
                checks.append(f'{locale}:{width}:localized controls reachable')
        collection.assert_clean(page, errors)
        calls = page.evaluate('__collection.calls')
        assert not [c for c in calls if c['command'] in ('native_download_start','native_backend_start','native_audio_enhance_install','native_job_submit')]
        result={'status':'PASS','checks':checks,'submissions':page.evaluate('__collection.aceSubmissions'),'screenshots':str(shots),'kind':'Chromium; strict simulated IPC, no actual model generation'}
        (shots/'ace-music-result.json').write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps(result,indent=2))
    except Exception:
        page.screenshot(path=str(shots/'failure.png'))
        raise
    finally:
        context.close()


if __name__ == '__main__':
    runner.verify = verify
    runner.main()
