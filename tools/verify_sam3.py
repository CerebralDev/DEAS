"""SAM editor browser/IPC fixtures, real canvas masks; not model-inference evidence."""
import ast
import sys
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[1]
URL = (sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:1421').rstrip('/')
OUT = Path(sys.argv[2]) if len(sys.argv) > 2 else Path.home()/'.hermes/cache/scratch/deas-sam-browser'
OUT.mkdir(parents=True, exist_ok=True)
module = ast.parse((ROOT/'tools/verify_inpaint.py').read_text())
INIT = next(ast.literal_eval(node.value) for node in module.body if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'INIT' for t in node.targets))
SAM = r"""
const fallback=window.__TAURI_INTERNALS__.invoke;
window.__samJobs=[];window.__refines=[];
window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
 if(command==='native_media_star_status')return {starred:false};
 if(command==='native_media_star_notice')return null;
 if(command==='native_wildcard_library')return {categories:[]};
 if(command==='native_audio_enhance_capabilities')return {};
 if(command==='native_recipe_prerequisites'){
  const result=await fallback(command,args);
  result.allowlist.nodes.push('SAM3_Detect','PrimitiveBoundingBox');
  result.allowlist.checkpoints.push('SAM3/sam3.1_multiplex_fp16.safetensors');return result;
 }
 if(command==='native_workbench_current'||command==='native_workbench_poll')return {jobs:window.__samJobs,preparing:0,capacity:20,stopAvailable:true,admissionEpoch:0,stopping:false};
 if(command==='native_workbench_run'&&args.intent.recipeId==='sam3-mask'){
  window.__calls.push({command,args});window.__plans.push(structuredClone(args.intent));
  const job={id:'sam-'+window.__samJobs.length,baseUrl:args.baseUrl,promptId:'p',state:'succeeded',outputs:[{filename:'mask.png',subfolder:'',type:'output'}],error:null,generationReady:false,intent:structuredClone(args.intent)};
  window.__samJobs.push(job);return job;
 }
 if(command==='native_job_output'){
  const c=document.createElement('canvas');c.width=1024;c.height=640;const x=c.getContext('2d');x.fillStyle='#000';x.fillRect(0,0,1024,640);x.fillStyle='#fff';x.fillRect(200,100,100,80);
  const blob=await new Promise(resolve=>c.toBlob(resolve,'image/png'));const bytes=Array.from(new Uint8Array(await blob.arrayBuffer()));
  return {jobId:args.jobId,index:0,mimeType:'image/png',width:1024,height:640,sha256:'a'.repeat(64),bytes};
 }
 if(command==='native_auto_refine'){window.__refines.push(structuredClone(args));return {job:null,skipped:0};}
 return fallback(command,args);
};
"""
with sync_playwright() as p:
    browser = p.chromium.launch()
    errors=[]
    page=browser.new_page(viewport={'width':1440,'height':960},reduced_motion='reduce')
    page.on('pageerror',lambda e:errors.append(str(e)))
    page.add_init_script(INIT+'\n'+SAM)
    page.goto(URL+'/tools/inpaint-fixture.html',wait_until='networkidle')
    page.get_by_role('button',name='base.png',exact=True).click()
    page.locator('[data-use-input]:visible').first.click()
    page.get_by_role('button',name='Inpaint',exact=True).click()
    page.locator('[data-sam="model"]').select_option('SAM3/sam3.1_multiplex_fp16.safetensors')
    page.locator('[data-sam="targets"]').fill('eyes, feet, hands')
    page.locator('[data-sam="mask"]').click()
    try:
        page.wait_for_function('window.__plans.length===3')
    except Exception:
        print('ERRORS',errors,'PLANS',page.evaluate('window.__plans'),'CALLS',page.evaluate('window.__calls.map(c=>({command:c.command,args:c.args.png?{...c.args,png:c.args.png.length}:c.args}))'),'BODY',page.inner_text('body'))
        page.screenshot(path=str(OUT/'failed.png'),full_page=True)
        raise
    expect(page.get_by_role('button',name='hands',exact=True)).to_be_visible()
    assert page.evaluate('window.__plans.map(i=>i.prompt)')==['eyes','feet','hands']
    # New layers preserve the initial mask and black mask background is transparent.
    masks=page.evaluate('''()=>[...document.querySelectorAll('.sie-rail canvas')].map(c=>c.width)''')
    assert len(masks)>=4,masks
    page.locator('[data-sam="refine"]').click()
    page.wait_for_function('window.__refines.length===1')
    refine=page.evaluate('window.__refines[0]')
    assert refine['settings']=={'checkpoint':'SAM3/sam3.1_multiplex_fp16.safetensors','targets':[],'denoise':0.25,'margin':32,'threshold':0.5},refine['settings']
    assert [m['name'] for m in refine['masks']]==['eyes','feet','hands']
    assert all(m['png'][:8]==[137,80,78,71,13,10,26,10] for m in refine['masks'])
    # Point and box travel through the same fitted source-pixel transform as brush paint.
    page.locator('[data-sam-tool="samPoint"]').click()
    r=page.get_by_test_id('editor-world').bounding_box();assert r
    page.mouse.click(r['x']+r['width']*.25,r['y']+r['height']*.25)
    page.wait_for_function('window.__plans.length===4')
    point=page.evaluate('window.__plans.at(-1).positivePoints[0]')
    assert abs(point['x']-256)<=1 and abs(point['y']-160)<=1,point
    page.wait_for_function('!document.querySelector("[data-sam-tool=samBox]").disabled')
    page.locator('[data-sam-tool="samBox"]').click()
    r=page.get_by_test_id('editor-world').bounding_box();assert r
    page.mouse.move(r['x']+r['width']*.2,r['y']+r['height']*.2);page.mouse.down()
    page.mouse.move(r['x']+r['width']*.4,r['y']+r['height']*.4);page.mouse.up()
    page.wait_for_function('window.__plans.length===5')
    box=page.evaluate('window.__plans.at(-1).box')
    assert abs(box['x']-205)<=1 and abs(box['y']-128)<=1 and abs(box['width']-205)<=1 and abs(box['height']-128)<=1,box
    page.wait_for_function('!document.querySelector("[data-sam=refine]").disabled')
    for width in (1440,768,390):
        page.set_viewport_size({'width':width,'height':960})
        for locale in ('en','de','fr','ja','tr','zh'):
            page.get_by_role('combobox',name='Fixture language',exact=True).select_option(locale)
            assert not page.evaluate('document.documentElement.scrollWidth>innerWidth'),(width,locale)
        page.get_by_role('combobox',name='Fixture language',exact=True).select_option('en')
        page.screenshot(path=str(OUT/f'sam-editor-{width}.png'),full_page=True)
    assert not errors,errors
    assert page.evaluate('window.__unexpected')==[],page.evaluate('window.__unexpected')
    browser.close()
print('PASS: text group masks, layer retention, manual native chain, source-pixel point/box, six locales/three widths')
