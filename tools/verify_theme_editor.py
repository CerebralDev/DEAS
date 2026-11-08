"""Actual Chromium offline editor; browser directory handles are a strict in-memory fixture."""
import base64,json,struct,tempfile,zlib
from pathlib import Path
from playwright.sync_api import expect
import verify_postprocessing as runner

def png_bytes():
    raw = b'\x00\xff\x00\x00\xff'
    def chunk(kind, payload):
        return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload) & 0xffffffff)
    ihdr = struct.pack('>IIBBBBB', 1, 1, 8, 6, 0, 0, 0)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', ihdr) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b'')

FIXTURE=r'''
window.__themeFiles={'theme.json':__MANIFEST__};window.__themeDirs=new Set(['']);window.__textReads=[];
const missing=()=>new DOMException('File not found','NotFoundError');
function directory(prefix=''){
 return {name:prefix||'sage',kind:'directory',
 async getDirectoryHandle(name,options={}){if(!/^[a-z0-9-]+$/.test(name))throw Error('Unexpected directory');const key=prefix+name+'/';if(!__themeDirs.has(key)){if(!options.create)throw missing();__themeDirs.add(key);}return directory(key);},
 async getFileHandle(name,options={}){
  if(name.includes('/')||name.includes('\\'))throw Error('Unexpected file path');const key=prefix+name;
  if(!Object.hasOwn(__themeFiles,key)){if(!options.create)throw missing();__themeFiles[key]='';}
  return {kind:'file',name,async getFile(){const file=new File([__themeFiles[key]],name);const text=file.text.bind(file);file.text=async()=>{(__textReads??=[]).push(key);return text();};return file;},async createWritable(){let pending;return {async write(value){pending=value;},async close(){if(window.__beforeClose)await __beforeClose(key);__themeFiles[key]=pending;},async abort(){}};}};
 }};
}
window.showDirectoryPicker=async()=>directory();
'''

def regressions(page,root,manifest,checks):
 png=png_bytes()
 payload={'name':'replacement.png','mimeType':'image/png','buffer':png}
 page.evaluate('value=>window.__baseline=value',manifest)
 page.evaluate('value=>window.__png=value',list(png))
 def reset(shared=False):
  page.evaluate("""async shared=>{window.__beforeClose=null;__themeFiles={'theme.json':__baseline};__themeDirs=new Set(['','assets/']);__textReads=[];
   const value=JSON.parse(__baseline);if(shared){value.assets={background:'assets/shared.png',cursor:'assets/shared.png'};__themeFiles['assets/shared.png']=new File([new Uint8Array(__png)],'shared.png');}
   __themeFiles['theme.json']=JSON.stringify(value);await load(__themeFiles['theme.json'],await showDirectoryPicker());} """,shared)
 def save():
  page.get_by_role('button',name='Save changes',exact=True).click()
 def select():
  page.locator('[data-asset=background]').set_input_files(payload)
  expect(page.locator('#status')).to_contain_text('Image ready')
 reset(True);select()
 # Pause after the replacement asset starts committing, not merely before save begins.
 page.evaluate("""()=>{window.__held=false;__beforeClose=async key=>{if(key.startsWith('assets/')){__held=true;await new Promise(resolve=>window.__resume=resolve);}};}""")
 save();page.wait_for_function('()=>window.__held')
 for selector in ['#name','#id','#author','#color-ok','[data-asset=background]']:
  expect(page.locator(selector)).to_be_disabled()
 expect(page.get_by_role('button',name='Remove mouse cursor',exact=True)).to_be_disabled()
 expect(page.locator('#open')).to_be_disabled();expect(page.locator('#undo')).to_be_disabled()
 page.evaluate('window.__resume()');expect(page.locator('#status')).to_contain_text('Saved.')
 assert page.evaluate("JSON.parse(__themeFiles['theme.json']).assets.cursor==='assets/shared.png' && JSON.parse(__themeFiles['theme.json']).assets.background!=='assets/shared.png'")
 assert page.evaluate("async()=>Array.from(new Uint8Array(await __themeFiles['assets/shared.png'].arrayBuffer())).join()===__png.join()")
 expect(page.locator('#name')).to_be_enabled();expect(page.locator('#dirty')).to_have_text('')
 checks.append('delayed asset write locks all editing/undo/open controls; shared role asset bytes remain intact')
 for fail in ['external','failure']:
  reset(True);select();previous=page.evaluate("__themeFiles['theme.json']")
  page.evaluate("""mode=>{__beforeClose=async key=>{if(key.startsWith('assets/')){if(mode==='external')__themeFiles['theme.json']+='\\n';else throw Error('injected asset write failure');}};}""",fail)
  save();expect(page.locator('#status')).to_contain_text('Could not save')
  if fail=='external':expect(page.locator('#status')).to_contain_text('changed outside this editor')
  assert page.evaluate("__themeFiles['theme.json']")==previous+('\n' if fail=='external' else '')
  assert page.evaluate("async()=>Array.from(new Uint8Array(await __themeFiles['assets/shared.png'].arrayBuffer())).join()===__png.join()")
  expect(page.locator('#dirty')).to_have_text('Unsaved changes');expect(page.locator('#name')).to_be_enabled()
 checks.append('external manifest edit during asset write is preserved; failed asset write retains prior manifest and referenced images')
 # A failed manifest commit likewise cannot destroy any asset referenced by the prior manifest.
 reset(True);select();previous=page.evaluate("__themeFiles['theme.json']")
 page.evaluate("__beforeClose=async key=>{if(key==='theme.json')throw Error('injected manifest failure');}")
 save();expect(page.locator('#status')).to_contain_text('injected manifest failure')
 assert page.evaluate("__themeFiles['theme.json']")==previous
 assert page.evaluate("async()=>Array.from(new Uint8Array(await __themeFiles['assets/shared.png'].arrayBuffer())).join()===__png.join()")
 checks.append('failed manifest commit leaves shared prior assets intact (new unreferenced images may remain)')
 reset();select()
 page.evaluate("""()=>{__themeFiles['assets/background-collision.png']='keep-existing-bytes';window.__uuid=crypto.randomUUID.bind(crypto);let first=true;crypto.randomUUID=()=>{if(first){first=false;return 'collision';}return __uuid();};}""")
 save();expect(page.locator('#status')).to_contain_text('Saved.')
 assert page.evaluate("__themeFiles['assets/background-collision.png']==='keep-existing-bytes'")
 page.evaluate('()=>{crypto.randomUUID=__uuid;}')
 checks.append('fresh asset allocation retries an existing filename without overwriting its bytes')
 # Hold just one decoder, allowing the next selection/save/undo/open to complete first.
 for action in ['selection','undo','open','save']:
  reset();page.locator('#name').fill('Dirty before inspect')
  page.evaluate("""()=>{window.__decode=createImageBitmap;window.__decodeHeld=false;window.__decodeDone=false;window.createImageBitmap=async file=>{if(file.name==='slow.png'){__decodeHeld=true;await new Promise(resolve=>window.__decodeResume=resolve);const bitmap=await __decode(file);__decodeDone=true;return bitmap;}return __decode(file);};}""")
  page.locator('[data-asset=background]').set_input_files({**payload,'name':'slow.png'})
  page.wait_for_function('()=>window.__decodeHeld')
  if action=='selection':select()
  elif action=='undo':page.locator('#undo').click()
  elif action=='open':page.locator('#open').click();expect(page.locator('#name')).to_have_value('New theme');expect(page.locator('#status')).to_contain_text('Folder opened')
  else:save();expect(page.locator('#status')).to_contain_text('Saved.')
  before=page.evaluate('JSON.stringify(model)');before_status=page.locator('#status').inner_text()
  page.evaluate('window.__decodeResume()');page.wait_for_function('()=>window.__decodeDone');page.wait_for_timeout(30)
  assert page.evaluate('JSON.stringify(model)')==before,action
  assert page.locator('#status').inner_text()==before_status,action
  page.evaluate('()=>{window.createImageBitmap=__decode;}')
 checks.append('stale async image inspection cannot override a newer selection, undo, folder open or completed save')
 reset()
 page.evaluate("""()=>{window.__decode=createImageBitmap;window.__decodeHeld=false;window.createImageBitmap=async file=>{if(file.name==='slow.png'){__decodeHeld=true;await new Promise(resolve=>window.__decodeResume=resolve);}return __decode(file);};const value=JSON.parse(__baseline);value.assets={background:'assets/slow.png'};__themeFiles['assets/slow.png']=new File([new Uint8Array(__png)],'slow.png');__themeFiles['theme.json']=JSON.stringify(value);window.__rendering=showDirectoryPicker().then(handle=>load(__themeFiles['theme.json'],handle));}""")
 page.wait_for_function('()=>window.__decodeHeld');select()
 preview=page.locator('#preview').evaluate('el=>el.style.backgroundImage')
 page.evaluate('async()=>{__decodeResume();await __rendering;window.createImageBitmap=__decode;}')
 assert page.locator('#preview').evaluate('el=>el.style.backgroundImage')==preview
 expect(page.locator('#dirty')).to_have_text('Unsaved changes')
 checks.append('delayed existing-image preview cannot replace a newly selected image preview')
 reset()
 invalid=[]
 for key,values in {'id':[42,['sage'],None], 'base':[['sage'],None,42], 'name':['bad\u0085name','bad\u202ename','bad\u2066name'], 'author':['bad\u009fname','bad\u200bname']}.items():
  for value in values:invalid.append({**json.loads(manifest),key:value})
 assert page.evaluate("values=>values.every(value=>{try{validate(value);return false;}catch{return true;}})",invalid)
 assert page.evaluate("value=>{validate(value);return true;}",json.loads(manifest))
 # Exercise the read bound on all actual filesystem manifest read paths, including the late check.
 for stage in ['open','initial-save','late-save']:
  reset();page.evaluate('__textReads=[]')
  if stage=='open':
   page.evaluate("__themeFiles['theme.json']=' '.repeat(65537)");page.locator('#open').click()
  elif stage=='initial-save':
   page.locator('#name').fill('Bound test');page.evaluate("__themeFiles['theme.json']=' '.repeat(65537)");save()
  else:
   select();page.evaluate("()=>{__beforeClose=async key=>{if(key.startsWith('assets/')){__themeFiles['theme.json']=' '.repeat(65537);__textReads=[];}};}");save()
  expect(page.locator('#status')).to_contain_text('maximum 64 KiB')
  assert page.evaluate('__textReads.length')==0,stage
 assert page.evaluate("""async()=>{const file=new File(['é'.repeat(32769)],'theme.json');file.text=()=>{throw Error('text was read');};try{await manifestText(file);return false;}catch(error){return error.message.includes('64 KiB');}}""")
 assert page.evaluate("""async()=>{try{await load(' '.repeat(65536)+'{}',null);return false;}catch(error){return error.message.includes('64 KiB');}}""")
 checks.append('strict id/base types, Unicode Cc/Cf rejection, 64 KiB byte bounds before open/early-save/late-save text reads')
 reset()
 result=page.evaluate("""async()=>{const valid=new Uint8Array(__png);const huge=valid.slice();new DataView(huge.buffer).setUint32(16,50000);
  const webp=new Uint8Array(30);webp.set([82,73,70,70,22,0,0,0,87,69,66,80,86,80,56,88,10,0,0,0]);webp.set([255,255,255],24);webp[27]=1;
  const original=createImageBitmap;let decoded=0;window.createImageBitmap=async()=>{decoded++;throw Error('unexpected decode');};
  const failures=[];for(const [bytes,path]of [[huge,'huge.png'],[webp,'huge.webp'],[valid,'wrong.webp']]){try{await inspectImage(new File([bytes],path),path);failures.push('accepted');}catch(error){failures.push(error.message);}}
  window.createImageBitmap=original;return {decoded,failures};}""")
 assert result['decoded']==0,result
 assert all('too large' in msg for msg in result['failures'][:2]),result
 assert 'extension must match' in result['failures'][2],result
 # Existing invalid files must never reach blob URLs / preview decoders.
 page.evaluate("""async()=>{const value=JSON.parse(__baseline);value.assets={background:'assets/wrong.webp'};__themeFiles['assets/wrong.webp']=new File([new Uint8Array(__png)],'wrong.webp');__themeFiles['theme.json']=JSON.stringify(value);await load(__themeFiles['theme.json'],await showDirectoryPicker());}""")
 assert page.locator('#preview').evaluate("el=>el.style.backgroundImage")==''
 expect(page.locator('#assets')).to_contain_text('extension must match')
 page.locator('#name').fill('Invalid existing image');save();expect(page.locator('#status')).to_contain_text('extension must match')
 checks.append('PNG/WebP header dimensions rejected before decode; existing mismatched-extension images rejected before preview and save')


HOST=r"""<!doctype html><meta charset=utf-8><title>DEAS host fixture</title>
<iframe id="editor" src="__EDITOR__" style="width:1400px;height:900px;border:0"></iframe>
<script>
const files={'theme.json':__MANIFEST__};window.__files=files;window.__reads=[];window.__writes=[];
const encoded=value=>btoa(unescape(encodeURIComponent(value)));
addEventListener('message',async event=>{const request=event.data;if(!request||request.source!=='deas-theme-editor')return;
 const reply={source:'deas-theme-host',id:request.id};
 if(request.op==='folders')reply.folders=['my-theme'];
 else if(request.op==='read'){window.__reads.push(request.path);reply.base64=Object.hasOwn(files,request.path)?encoded(files[request.path]):null;}
 else if(request.op==='write'){window.__writes.push(request.path);files[request.path]=decodeURIComponent(escape(atob(request.base64)));reply.written=true;}
 else reply.error='Unknown request';
 event.source.postMessage(reply,'*');});
</script>"""


def embedded(browser,root,manifest,checks,errors):
 """The editor inside DEAS: a host fixture stands in for the native theme file bridge."""
 with tempfile.TemporaryDirectory() as temporary:
  page_file=Path(temporary)/'host.html'
  page_file.write_text(HOST.replace('__EDITOR__',(root/'theme-packs/EDIT-THEMES.html').as_uri()).replace('__MANIFEST__',json.dumps(manifest)),encoding='utf-8')
  context=browser.new_context(viewport={'width':1440,'height':960});page=context.new_page()
  page.on('pageerror',lambda error:errors.append('embedded: '+str(error)))
  page.on('dialog',lambda dialog:dialog.accept())
  try:
   page.goto(page_file.as_uri(),wait_until='networkidle')
   frame=page.frame_locator('#editor')
   expect(frame.locator('#open')).to_be_hidden();expect(frame.locator('#choose')).to_be_visible()
   expect(frame.locator('#theme')).to_have_js_property('options.length',2)
   frame.locator('#theme').select_option('my-theme')
   expect(frame.locator('#name')).to_have_value('New theme');expect(frame.locator('#status')).to_contain_text('Folder opened')
   frame.locator('#name').fill('Environment theme')
   frame.get_by_role('button',name='Save changes',exact=True).click()
   expect(frame.locator('#status')).to_contain_text('Saved.')
   saved=page.evaluate("JSON.parse(__files['theme.json'])")
   assert saved['name']=='Environment theme',saved
   assert any(path.startswith('backups/theme-') for path in page.evaluate('__writes')),page.evaluate('__writes')
   assert 'theme.json' in page.evaluate('__reads')
   checks.append('framed editor lists environment themes and saves manifest plus backup through the native bridge fixture')
  finally:context.close()


def verify(browser,url,shots):
 root=Path(__file__).resolve().parents[1]
 manifest=(root/'theme-packs/new-theme/theme.json').read_text()
 checks=[];errors=[]
 canonical=(root/'theme-packs/EDIT-THEMES.html').read_bytes()
 copies=list((root/'theme-packs').glob('*/themes/*/EDIT-THEME.html'))
 assert canonical and not copies
 checks.append('one canonical offline editor; built-in themes are not duplicated as pack editors')
 context=browser.new_context(viewport={'width':1440,'height':960})
 page=context.new_page();page.on('pageerror',lambda error:errors.append(str(error)))
 page.on('dialog',lambda dialog:dialog.accept())
 page.add_init_script(FIXTURE.replace('__MANIFEST__',json.dumps(manifest)))
 try:
  page.goto((root/'theme-packs/EDIT-THEMES.html').as_uri(),wait_until='networkidle')
  page.get_by_role('button',name='Edit theme').click();expect(page.locator('#name')).to_have_value('New theme')
  page.locator('#name').fill('My custom theme');page.locator('#id').fill('my-custom-theme')
  page.get_by_role('textbox',name='Main buttons hex',exact=True).fill('#123abc')
  webp=page.evaluate("""async()=>{const canvas=document.createElement('canvas');canvas.width=16;canvas.height=16;canvas.getContext('2d').fillRect(0,0,16,16);const blob=await new Promise(r=>canvas.toBlob(r,'image/webp'));return btoa(String.fromCharCode(...new Uint8Array(await blob.arrayBuffer())));}""")
  page.locator('[data-asset=background]').set_input_files({'name':'my-picture.webp','mimeType':'image/webp','buffer':base64.b64decode(webp)})
  expect(page.locator('#status')).to_contain_text('Image ready')
  page.locator('[data-asset=cursor]').set_input_files({'name':'cursor.png','mimeType':'image/png','buffer':png_bytes()})
  expect(page.locator('#status')).to_contain_text('Image ready')
  page.get_by_role('button',name='Save changes',exact=True).click();expect(page.locator('#status')).to_contain_text('Saved.')
  written=page.evaluate('JSON.parse(__themeFiles["theme.json"])')
  assert written['id']=='my-custom-theme' and written['name']=='My custom theme'
  assert written['colors']['ok']=='#123abc' and written['base']=='default'
  assert set(written['assets'])=={'background','cursor'}
  assert written['assets']['background'].startswith('assets/background-') and written['assets']['background'].endswith('.webp')
  assert written['assets']['cursor'].startswith('assets/cursor-') and written['assets']['cursor'].endswith('.png')
  assert page.evaluate("Object.keys(__themeFiles).some(p=>p.startsWith('backups/theme-'))")
  assert page.evaluate("Object.values(JSON.parse(__themeFiles['theme.json']).assets).every(p=>__themeFiles[p] instanceof File)")
  checks.append('real DOM color/identity edits; PNG and WebP image selection; paths, bytes and manifest backup saved through directory fixture')
  for width in [1440,390]:
   page.set_viewport_size({'width':width,'height':960});assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'),width
   page.screenshot(path=str(shots/f'editor-{width}.png'),full_page=True)
  checks.append('desktop and narrow editor bounds')
  page.locator('#name').fill('Discard me');page.get_by_role('button',name='Undo unsaved changes').click();expect(page.locator('#name')).to_have_value('My custom theme')
  checks.append('undo unsaved changes')
  page.get_by_role('button',name='Remove background image',exact=True).click();page.get_by_role('button',name='Save changes',exact=True).click();expect(page.locator('#status')).to_contain_text('Saved.')
  assert 'background' not in page.evaluate('JSON.parse(__themeFiles["theme.json"]).assets')
  checks.append('remove an optional image and save')
  page.get_by_role('textbox',name='Main buttons hex',exact=True).fill('not-color');assert not page.get_by_role('textbox',name='Main buttons hex',exact=True).evaluate('e=>e.checkValidity()')
  page.get_by_role('textbox',name='Main buttons hex',exact=True).fill('#abcdef')
  page.evaluate("suffix=>__themeFiles['theme.json']+=suffix",'\n');previous=page.evaluate("__themeFiles['theme.json']")
  page.get_by_role('button',name='Save changes',exact=True).click();expect(page.locator('#status')).to_contain_text('changed outside this editor')
  assert page.evaluate("__themeFiles['theme.json']")==previous
  checks.append('invalid colors and concurrent external edits are not overwritten')
  page.get_by_role('button',name='Edit theme').click();expect(page.locator('#status')).to_contain_text('Folder opened')
  page.locator('[data-asset=background]').set_input_files({'name':'fake.png','mimeType':'image/png','buffer':b'not an image'})
  expect(page.locator('#status')).to_contain_text('Choose a PNG or static WebP')
  checks.append('invalid image rejection')
  regressions(page,root,manifest,checks)
  # Missing optional files never cause network requests; the editor has no connector.
  assert not errors,errors
 finally:context.close()
 fallback=browser.new_context(accept_downloads=True);page=fallback.new_page()
 page.add_init_script('window.showDirectoryPicker=undefined')
 try:
  page.goto((root/'theme-packs/EDIT-THEMES.html').as_uri(),wait_until='networkidle')
  with page.expect_file_chooser() as chooser:page.get_by_role('button',name='Edit theme').click()
  chooser.value.set_files(root/'theme-packs/new-theme/theme.json')
  expect(page.locator('#status')).to_contain_text('Preview mode')
  page.locator('#name').fill('Downloaded theme')
  with page.expect_download() as download:page.get_by_role('button',name='Save changes',exact=True).click()
  assert json.loads(Path(download.value.path()).read_text())['name']=='Downloaded theme'
  expect(page.locator('[data-asset=background]')).to_be_disabled()
  checks.append('browser without directory access downloads a valid manifest and disables image replacement')
  page.evaluate("""()=>{window.__fallbackReads=0;const text=File.prototype.text;File.prototype.text=function(){__fallbackReads++;return text.call(this);};}""")
  page.locator('#fallback').set_input_files({'name':'theme.json','mimeType':'application/json','buffer':b' '*65537})
  expect(page.locator('#status')).to_contain_text('maximum 64 KiB')
  assert page.evaluate('window.__fallbackReads')==0
  checks.append('fallback manifest upload also rejects oversized bytes before reading text')
 finally:fallback.close()
 embedded(browser,root,manifest,checks,errors)
 result={'status':'PASS','kind':'Actual Chromium; offline editor save uses strict directory-handle fixture, native browser folder permission remains a manual gate','checks':checks,'screenshots':str(shots)}
 (shots/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2),flush=True)

if __name__=='__main__':runner.verify=verify;runner.main()
