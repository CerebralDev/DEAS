import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent))
import verify_postprocessing as runner
import verify_workbench_collection as c
import verify_workspace_layout as w
c.FIXTURE += r"""
const workspaceInvoke=window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke=async(c,a)=>{
 if(c==='native_model_library_read')return {schemaVersion:1,revision:0,folders:[],entries:{},order:[],views:[]};
 if(c==='native_model_assets')return {};
 if(c==='native_spellcheck_check')return [];
 if(c==='native_audio_enhance_capabilities')return {clean:true,deep:false,stems:false};
 return workspaceInvoke(c,a);
};
"""
def verify(browser,url,shots):
 c.mixed_case(browser,url,shots)
 c.empty_case(browser,url)
 c.rooted_case(browser,url)
 print('PAIRED_WORKBENCH_PASS',flush=True)
runner.verify=verify
runner.main()
