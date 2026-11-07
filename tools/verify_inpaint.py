"""Real Workbench + real browser canvases; IPC fixture, not GPU generation."""
from pathlib import Path
import sys
from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[1]
URL = (sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:1421').rstrip('/')
INIT = r"""
localStorage.clear(); localStorage.setItem('deas.locale.v1','en');
window.isTauri=true; window.__calls=[]; window.__uploads=[]; window.__plans=[]; window.__unexpected=[];
window.__holdUpload=false; window.__releaseUpload=null;
const png=(w,h,color)=>{const c=document.createElement('canvas');c.width=w;c.height=h;const x=c.getContext('2d');x.fillStyle=color;x.fillRect(0,0,w,h);return c.toDataURL('image/png');};
const images={'base.png':png(1024,640,'#555555'),'cat.png':png(200,100,'#ff0000'),'other.png':png(1024,640,'#333333'),'odd.png':png(1000,600,'#777777')};
const maskName='deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png';
let callback=0;
window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>++callback,
 convertFileSrc:path=>images[path.split('/').pop()]||images['base.png'],
 invoke:async(command,args={})=>{
  window.__calls.push({command,args});
  if(command==='plugin:event|listen')return ++callback;
  if(command==='plugin:event|unlisten')return null;
  if(command==='native_media_thumbnail')return args.path;
  if(command==='native_media_metadata')return [];
  if(command==='native_media_list')return {path:args.path,parent:'/env/ComfyUI',entries:Object.keys(images).map((name,i)=>({name,path:args.path+'/'+name,kind:'file',symlink:false,media:'image',size:1,modified:100-i})),truncated:false};
  if(command==='native_models_scan')return [{category:'checkpoints',name:'sdxl.safetensors',path:'/env/ComfyUI/models/checkpoints/sdxl.safetensors',size:1,info:null,family:'sdxl'}];
  if(command==='native_recipe_prerequisites')return {baseUrl:args.baseUrl,recipeId:'checkpoint-text-to-image-diagnostic',version:1,missingNodes:[],generationReady:false,allowlist:{nodes:['CheckpointLoaderSimple','CLIPTextEncode','EmptyLatentImage','KSampler','VAEDecode','SaveImage','LoadImage','VAEEncode','LoadImageMask','SetLatentNoiseMask','ImageCompositeMasked','ImageScale','ImageCrop','CropMask','MaskToImage','ImageToMask','ImageBlur'],checkpoints:['sdxl.safetensors'],loras:[],samplers:['euler'],schedulers:['normal']}};
  if(command==='native_mask_upload'){
   if(!Array.isArray(args.png)||args.png.length<8)throw Error('No real PNG upload');
   window.__uploads.push([...args.png]);
   if(window.__holdUpload)await new Promise(resolve=>{window.__releaseUpload=resolve;});
   return maskName;
  }
  if(command==='native_workbench_current'||command==='native_workbench_poll')return {jobs:window.__job?[window.__job]:[],preparing:0,capacity:20,stopAvailable:false};
  if(command==='native_workbench_run'){window.__plans.push(structuredClone(args.intent));return window.__job={id:'job-'+window.__plans.length,baseUrl:'http://127.0.0.1:7801',promptId:'p',state:'failed',outputs:[],error:'fixture: no generation',generationReady:false,intent:window.__plans.at(-1)};}
  window.__unexpected.push(command);throw Error('Unexpected IPC '+command);
 }};
"""


def open_editor(page):
    page.goto(URL + '/tools/inpaint-fixture.html', wait_until='networkidle')
    page.get_by_role('button', name='base.png', exact=True).click()
    page.locator('[data-use-input]').click()
    page.get_by_role('button', name='Inpaint', exact=True).click()
    page.locator('[data-editor-viewport]').wait_for()
    expect(page.get_by_role('button', name='Mask 1', exact=True)).to_have_attribute('aria-pressed', 'true')


def click_tool(page, name):
    page.get_by_role('button', name=name, exact=True).click()


def drop_tile(page, name):
    """HTML5 drag of a dock tile onto the editor viewport (the only way to insert a layer)."""
    page.get_by_role('button', name=name, exact=True).drag_to(page.locator('[data-editor-viewport]'))


def paint(page, x, y, end=None):
    r = page.get_by_test_id('editor-world').bounding_box()
    assert r and r['width'] > 0 and r['height'] > 0
    # Some source points can be outside the viewport after 100%; use Fit before painting.
    px = r['x'] + x * r['width'] / 1024
    py = r['y'] + y * r['height'] / 640
    page.mouse.move(px, py)
    page.mouse.down()
    if end:
        page.mouse.move(r['x'] + end[0] * r['width'] / 1024,
                        r['y'] + end[1] * r['height'] / 640, steps=2)
    page.mouse.up()


def pixels(page, points, index=-1):
    return page.evaluate('''async ({points,index})=>{
      const bytes=window.__uploads.at(index),blob=new Blob([new Uint8Array(bytes)],{type:'image/png'}),url=URL.createObjectURL(blob);
      try{const img=new Image();img.src=url;await img.decode();const c=document.createElement('canvas');c.width=img.naturalWidth;c.height=img.naturalHeight;
        const ctx=c.getContext('2d');ctx.drawImage(img,0,0);const d=ctx.getImageData(0,0,c.width,c.height).data;
        return {width:c.width,height:c.height,opaque:d.every((v,i)=>i%4!==3||v===255),values:points.map(([x,y])=>Array.from(d.slice((y*c.width+x)*4,(y*c.width+x)*4+4)))};
      }finally{URL.revokeObjectURL(url);}
    }''', {'points': points, 'index': index})


def export_mask(page):
    before = page.evaluate('window.__plans.length')
    canvases = page.locator('.sie-mask, .sie-paint canvas').element_handles()
    snapshots = [canvas.evaluate('(c)=>c.toDataURL()') for canvas in canvases]
    click_tool(page, 'Run')
    page.wait_for_function('(n)=>window.__plans.length===n+1', arg=before)
    # Edit jobs open side by side: the live editor stays visible, no return step.
    expect(page.locator('.wb-stage.wb-split [data-editor-viewport]')).to_be_visible()
    expect(page.locator('[data-editor-return]')).to_have_count(0)
    for canvas, snapshot in zip(canvases, snapshots):
        assert canvas.evaluate('(c)=>c.isConnected'), 'Split view replaced a painted canvas'
        assert canvas.evaluate('(c)=>c.toDataURL()') == snapshot, 'Split view lost painted pixels'
    return page.evaluate('window.__plans.at(-1)')


def geometry(page):
    return page.locator('.sie-image:not([data-layer="base"])').evaluate('(el)=>Object.fromEntries(["left","top","width","height"].map(k=>[k,parseFloat(el.style[k])]))')


def drag_image(page, dx, dy):
    world = page.get_by_test_id('editor-world').bounding_box()
    image = page.locator('.sie-image:not([data-layer="base"])').bounding_box()
    assert world and image
    scale = world['width'] / 1024
    page.mouse.move(image['x'] + image['width'] / 2, image['y'] + image['height'] / 2)
    page.mouse.down()
    page.mouse.move(image['x'] + image['width'] / 2 + dx * scale,
                    image['y'] + image['height'] / 2 + dy * scale, steps=3)
    page.mouse.up()


def resize_image(page, dx):
    world = page.get_by_test_id('editor-world').bounding_box()
    handle = page.locator('.sie-resize').bounding_box()
    assert world and handle
    scale = world['width'] / 1024
    page.mouse.move(handle['x'] + handle['width'] / 2, handle['y'] + handle['height'] / 2)
    page.mouse.down()
    page.mouse.move(handle['x'] + handle['width'] / 2 + dx * scale,
                    handle['y'] + handle['height'] / 2, steps=3)
    page.mouse.up()


with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page(viewport={'width': 1440, 'height': 960}, reduced_motion='reduce')
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    page.add_init_script(INIT)
    open_editor(page)
    click_tool(page, 'Fit')
    vp = page.locator('[data-editor-viewport]').bounding_box()
    assert vp
    cursor = (vp['x'] + vp['width'] * .6, vp['y'] + vp['height'] * .4)
    before = page.get_by_test_id('editor-world').bounding_box()
    assert before
    original = ((cursor[0] - before['x']) * 1024 / before['width'],
                (cursor[1] - before['y']) * 640 / before['height'])
    page.mouse.move(*cursor)
    page.mouse.wheel(0, -300)
    page.wait_for_function('(w)=>document.querySelector("[data-testid=editor-world]").getBoundingClientRect().width>w', arg=before['width'])
    after = page.get_by_test_id('editor-world').bounding_box()
    assert after
    assert abs((cursor[0] - after['x']) * 1024 / after['width'] - original[0]) < .5
    assert abs((cursor[1] - after['y']) * 640 / after['height'] - original[1]) < .5
    click_tool(page, 'Fit')
    page.get_by_role('spinbutton', name='Brush diameter (px)', exact=True).fill('80')
    click_tool(page, 'Square')
    paint(page, 256, 160)
    page.get_by_role('slider', name='Opacity (%)', exact=True).fill('0')
    intent = export_mask(page)
    assert (intent['width'], intent['height']) == (1024, 640)
    assert 'inputResize' not in intent
    assert intent['inputMask'].startswith('deas-masks/')
    result = pixels(page, [[256, 160], [300, 160], [220, 124]])
    assert (result['width'], result['height']) == (1024, 640)
    assert result['opaque'] and result['values'] == [[255,255,255,255],[0,0,0,255],[255,255,255,255]], result
    assert intent['maskBlur'] == 4 and 'inpaintCrop' not in intent
    page.get_by_role('checkbox', name='Inpaint only masked', exact=True).check()
    page.get_by_role('spinbutton', name='Mask Shrink Grow (px)', exact=True).fill('32')
    page.get_by_role('spinbutton', name='Mask Blur (px)', exact=True).fill('8')
    cropped = export_mask(page)
    assert cropped['maskBlur'] == 8
    crop = cropped['inpaintCrop']
    assert (cropped['width'], cropped['height']) == (1024,640)
    assert crop['targetWidth'] % 64 == crop['targetHeight'] % 64 == 0
    assert crop['targetWidth'] * crop['targetHeight'] >= 900000
    # The context rectangle includes 32 context pixels and the 8-pixel blur support on every side.
    assert abs(crop['x'] - 176) <= 1 and abs(crop['y'] - 80) <= 1, crop
    assert 160 <= crop['width'] <= 162 and 160 <= crop['height'] <= 162, crop
    page.get_by_role('spinbutton', name='Mask Shrink Grow (px)', exact=True).fill('0')
    tight = export_mask(page)['inpaintCrop']
    assert tight['width'] == crop['width'] - 64 and tight['height'] == crop['height'] - 64
    page.get_by_role('checkbox', name='Inpaint only masked', exact=True).uncheck()
    blurred = export_mask(page)
    assert blurred['maskBlur'] == 8 and 'inpaintCrop' not in blurred
    page.get_by_role('spinbutton', name='Mask Blur (px)', exact=True).fill('0')
    click_tool(page, 'Clear')
    paint(page, 128, 320, (896, 320))
    export_mask(page)
    assert all(v[0] == 255 for v in pixels(page, [[x,320] for x in (128,256,384,512,640,768,896)])['values'])
    # Existing studio.run.edit.clear is exactly 'Clear' in English.
    click_tool(page, 'Clear')
    paint(page, 256, 160)
    click_tool(page, 'Add mask')
    paint(page, 768, 480)
    click_tool(page, 'Mask 1')
    click_tool(page, 'Erase')
    paint(page, 256, 160)
    export_mask(page)
    result = pixels(page, [[256,160],[768,480]])
    assert result['values'] == [[0,0,0,255],[255,255,255,255]], result
    click_tool(page, 'Hide Mask 2')
    expect(page.get_by_role('button', name='Run', exact=True)).to_be_disabled()
    click_tool(page, 'Show Mask 2')
    expect(page.get_by_role('button', name='Run', exact=True)).to_be_enabled()
    # Add an opaque image while the mask document stays alive.
    drop_tile(page, 'cat.png')
    expect(page.locator('.sie-rail').get_by_role('button', name='cat.png [output]', exact=True)).to_have_attribute('aria-pressed', 'true')
    assert page.locator('.sie-image.is-selected').count() == 1
    assert page.locator('.sie-resize').count() == 1
    # Reselecting the same source must preserve the entire draft, not just canvas pixels.
    editor_node = page.get_by_test_id('editor-world').element_handle()
    page.get_by_role('button',name='base.png',exact=True).click()
    page.locator('[data-use-input]').click()
    expect(page.locator('.sie-image:not([data-layer="base"])')).to_have_count(1)
    expect(page.get_by_role('button',name='Run',exact=True)).to_be_enabled()
    assert page.get_by_test_id('editor-world').evaluate('(el,old)=>el===old',editor_node)
    page.locator('.sie-image:not([data-layer="base"])').press('ArrowRight')
    # Make the handle reachable at 200% without relying on offscreen clicks (the 200 px insert keeps its original size).
    for _ in range(7):
        page.locator('.sie-image:not([data-layer="base"])').press('-')
    for zoom in ('fit','double'):
        click_tool(page, 'Fit' if zoom == 'fit' else '100%')
        if zoom == 'double':
            import math
            vp = page.locator('[data-editor-viewport]').bounding_box()
            page.mouse.move(vp['x']+vp['width']/2,vp['y']+vp['height']/2)
            page.mouse.wheel(0,-math.log(2)*1000)
            page.wait_for_function('document.querySelector("[data-testid=editor-world]").getBoundingClientRect().width>2000')
        old = geometry(page)
        drag_image(page, 16, 8)
        moved = geometry(page)
        assert moved['left'] == old['left']+16 and moved['top'] == old['top']+8, (zoom, old, moved)
        resize_image(page, 16)
        resized = geometry(page)
        assert resized['width'] == moved['width']+16, (zoom, moved, resized)
        # Select interactions must not paint into either mask.
        assert resized['height'] > moved['height']
    click_tool(page, 'Fit')
    intent = export_mask(page)
    assert len(intent['inputLayers']) == 1
    layer = intent['inputLayers'][0]
    assert set(layer) == {'image','x','y','width','height'}
    assert layer['image'] == 'cat.png [output]'
    click_tool(page, 'Hide cat.png [output]')
    intent = export_mask(page)
    assert 'inputLayers' not in intent
    click_tool(page, 'Show cat.png [output]')
    # Snapshot must already exist while upload is held and editing is still possible.
    click_tool(page, 'Mask 2')
    click_tool(page, 'Brush')
    old_count = page.evaluate('window.__uploads.length')
    page.evaluate('window.__holdUpload=true')
    click_tool(page, 'Run')
    page.wait_for_function('(n)=>window.__uploads.length===n+1&&window.__releaseUpload!==null', arg=old_count)
    paint(page, 512, 320)
    snapshot = pixels(page, [[512,320]])
    assert snapshot['values'][0] == [0,0,0,255], snapshot
    page.evaluate('window.__holdUpload=false;window.__releaseUpload();window.__releaseUpload=null')
    expect(page.locator('.wb-stage.wb-split [data-editor-viewport]')).to_be_visible()
    # A model-invalid source must be explained, not silently rounded.
    page.get_by_role('button', name='odd.png', exact=True).click()
    page.locator('[data-use-input]').click()
    click_tool(page, 'Inpaint')
    expect(page.get_by_role('button', name='Run', exact=True)).to_be_disabled()
    expect(page.get_by_text('Resample this image to the selected model’s resolution grid before inpainting.', exact=True)).to_be_visible()
    assert page.evaluate('window.__unexpected') == []
    # Mask cap, delete/re-add, round pixels and cancellation use a fresh document.
    open_editor(page)
    for _ in range(3):
        click_tool(page, 'Add mask')
    expect(page.get_by_role('button',name='Add mask',exact=True)).to_be_disabled()
    for n in range(1,5):
        click_tool(page, f'Delete Mask {n}')
    click_tool(page, 'Add mask')
    expect(page.get_by_role('button',name='Mask 5',exact=True)).to_have_attribute('aria-pressed','true')
    page.get_by_role('spinbutton',name='Brush diameter (px)',exact=True).fill('80')
    click_tool(page, 'Round')
    click_tool(page, 'Fit')
    paint(page,256,160)
    export_mask(page)
    result=pixels(page,[[256,160],[220,124]])
    assert result['values']==[[255,255,255,255],[0,0,0,255]],result
    click_tool(page, 'Clear')
    click_tool(page, 'Square')
    # A cancelled stroke cannot continue to another part of the source.
    world=page.get_by_test_id('editor-world').bounding_box()
    viewport=page.locator('[data-editor-viewport]')
    viewport.evaluate('(el)=>el.addEventListener("pointerdown",e=>window.__pointerId=e.pointerId,{once:true})')
    page.mouse.move(world['x']+256*world['width']/1024,world['y']+160*world['height']/640)
    page.mouse.down()
    viewport.dispatch_event('pointercancel',{'pointerId':page.evaluate('window.__pointerId'),'pointerType':'mouse'})
    page.mouse.move(world['x']+768*world['width']/1024,world['y']+480*world['height']/640,steps=3)
    page.mouse.up()
    export_mask(page)
    assert pixels(page,[[768,480]])['values'][0]==[0,0,0,255]
    # Delay real Image decoding only for the drop probe; normal <img> elements still load.
    page.evaluate('''()=>{const NativeImage=window.Image,desc=Object.getOwnPropertyDescriptor(HTMLImageElement.prototype,'src');
      window.__probeDone=false;window.__restoreImage=()=>{window.Image=NativeImage;};
      window.Image=function(){const img=new NativeImage();img.addEventListener('load',()=>window.__probeDone=true);Object.defineProperty(img,'src',{set(url){window.__releaseProbe=()=>desc.set.call(img,url);}});return img;};
    }''')
    drop_tile(page,'cat.png')
    page.get_by_role('button',name='other.png',exact=True).click()
    page.locator('[data-use-input]').click()
    page.evaluate('window.__releaseProbe();window.__restoreImage()')
    page.wait_for_function('window.__probeDone')
    assert page.locator('.sie-image:not([data-layer="base"])').count()==0
    page.route('**/missing-insert.png',lambda route:route.fulfill(status=404,body=''))
    page.evaluate('''()=>{const NativeImage=window.Image,desc=Object.getOwnPropertyDescriptor(HTMLImageElement.prototype,'src');
      window.__restoreImage=()=>{window.Image=NativeImage;};
      window.Image=function(){const img=new NativeImage();Object.defineProperty(img,'src',{set(){desc.set.call(img,'/missing-insert.png');}});return img;};
    }''')
    drop_tile(page,'cat.png')
    expect(page.get_by_role('alert')).to_have_text('This image could not be loaded for insertion.')
    page.evaluate('window.__restoreImage()')
    assert page.locator('.sie-image:not([data-layer="base"])').count()==0
    assert not errors, errors
    # The base uses the same paint/transform stack; exported pixels must match the preview.
    open_editor(page)
    click_tool(page, 'Fit')
    paint(page, 128, 80)
    drop_tile(page, 'cat.png')
    click_tool(page, 'Base image')
    click_tool(page, 'Brush')
    click_tool(page, '#007aff')
    paint(page, 256, 160)
    click_tool(page, 'Select')
    world = page.get_by_test_id('editor-world').bounding_box()
    scale = world['width'] / 1024
    page.mouse.move(world['x'] + 100*scale, world['y'] + 100*scale)
    page.mouse.down()
    page.mouse.move(world['x'] + 132*scale, world['y'] + 116*scale, steps=3)
    page.mouse.up()
    base = page.locator('[data-layer="base"]')
    assert base.evaluate('(el)=>[parseFloat(el.style.left),parseFloat(el.style.top)]') == [32,16]
    intent = export_mask(page)
    assert 'inputLayers' not in intent and intent['inputImage'].startswith('deas-masks/')
    # Moving the base off the source frame outpaints the uncovered strip: white in the mask, neutral grey in the image.
    assert pixels(page, [[128,80],[0,0],[200,300]], -2)['values'] == [[255,255,255,255],[255,255,255,255],[0,0,0,255]]
    result = pixels(page, [[288,176],[512,320],[0,0]])
    assert result['values'] == [[0,122,255,255],[255,0,0,255],[128,128,128,255]], result
    assert (result['width'],result['height']) == (1024,640)
    # Actual HTML5 row drag raises the moved base above the inserted picture.
    rail = page.locator('.sie-rail')
    rail.get_by_role('button',name='Base image',exact=True).drag_to(rail.get_by_role('button',name='cat.png [output]',exact=True))
    assert page.locator('[data-layer]').evaluate_all('(els)=>els.map(el=>el.dataset.layer)').pop() == 'base'
    export_mask(page)
    assert pixels(page, [[512,320]])['values'] == [[85,85,85,255]]
    # Image strokes are snapshotted before a delayed mask upload, not when it completes.
    click_tool(page, 'Base image')
    click_tool(page, 'Brush')
    old_uploads = page.evaluate('window.__uploads.length')
    old_plans = page.evaluate('window.__plans.length')
    page.evaluate('window.__holdUpload=true')
    click_tool(page, 'Run')
    page.wait_for_function('(n)=>window.__uploads.length===n+1&&window.__releaseUpload!==null', arg=old_uploads)
    paint(page, 400, 400)
    page.evaluate('window.__holdUpload=false;window.__releaseUpload();window.__releaseUpload=null')
    page.wait_for_function('(n)=>window.__plans.length===n+1', arg=old_plans)
    assert pixels(page, [[400,400]])['values'] == [[85,85,85,255]]
    export_mask(page)
    assert pixels(page, [[400,400]])['values'] == [[0,122,255,255]]
    # Resize and keyboard movement share source-pixel geometry; clearing only removes paint.
    click_tool(page, 'Base image')
    base.press('+')
    assert base.evaluate('(el)=>parseFloat(el.style.width)') == 1032
    click_tool(page, 'Clear')
    export_mask(page)
    assert pixels(page, [[288,176]])['values'] == [[85,85,85,255]]
    click_tool(page, 'Hide Base image')
    export_mask(page)
    assert pixels(page, [[512,320],[128,80]])['values'] == [[255,0,0,255],[128,128,128,255]] # hidden base: the uncovered frame is outpainted (grey)
    click_tool(page, 'Show Base image')
    assert page.evaluate('window.__unexpected') == []
    assert not errors, errors
    page.close()
    # Layout smoke at all widths/locales; no fake screenshot files.
    for width in (1440,1024,768,390,320):
        page = browser.new_page(viewport={'width':width,'height':960}, reduced_motion='reduce')
        page.add_init_script(INIT)
        page.on('pageerror', lambda error: errors.append(str(error)))
        open_editor(page)
        for locale in ('de','en','fr','ja','tr','zh'):
            page.get_by_role('combobox', name='Fixture language', exact=True).select_option(locale)
            assert not page.evaluate('document.documentElement.scrollWidth>innerWidth'), (width,locale)
        page.get_by_role('combobox', name='Fixture language', exact=True).select_option('en')
        if width >= 1024:
            assert not page.evaluate('document.documentElement.scrollHeight>innerHeight+1'), width
        if width in (1440,320):
            drop_tile(page,'cat.png')
            expect(page.locator('.sie-image:not([data-layer="base"])')).to_have_count(1)
            click_tool(page,'Mask 1')
            click_tool(page,'Fit')
            paint(page,256,160,(384,240))
            page.screenshot(path=str(ROOT/'assets'/f'deas-inpaint-fixture-{width}.png'), full_page=True)
        assert page.evaluate('window.__unexpected') == []
        page.close()
    browser.close()
    assert not errors, errors
print('inpaint editor fixture ok')
