//! GTK-owned render contexts; only nonblocking libmpv commands/events on the UI thread.
use super::Action;
use gtk::prelude::*;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::{c_char, c_int, c_void, CStr, CString},
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};
use tauri::Manager;
type Ptr = *mut c_void;
#[repr(C)]
struct Param {
    kind: c_int,
    data: Ptr,
}
#[repr(C)]
struct Init {
    get: unsafe extern "C" fn(Ptr, *const c_char) -> Ptr,
    ctx: Ptr,
}
#[repr(C)]
struct Fbo {
    id: c_int,
    width: c_int,
    height: c_int,
    format: c_int,
}
#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    reply: u64,
    data: Ptr,
}
#[repr(C)]
struct Property {
    name: *const c_char,
    format: c_int,
    data: Ptr,
}
macro_rules! api {
 ($($name:ident: $ty:ty),* $(,)?) => {
 struct Api { _library:libloading::Library,$($name:$ty),* }
 impl Api { unsafe fn load()->Result<Self,String> {
 let library=libloading::Library::new(std::env::var_os("DEAS_LIBMPV").unwrap_or_else(||"libmpv.so.2".into())).map_err(|_|"Install libmpv.so.2 (or set DEAS_LIBMPV)".to_string())?;
 $(let $name=*library.get::<$ty>(concat!(stringify!($name),"\0").as_bytes()).map_err(|_|"Incompatible libmpv".to_string())?;)*
 Ok(Self{_library:library,$($name),*})
 } }
 };
}
api! {
 mpv_create:unsafe extern "C" fn()->Ptr,
 mpv_initialize:unsafe extern "C" fn(Ptr)->c_int,
 mpv_set_option_string:unsafe extern "C" fn(Ptr,*const c_char,*const c_char)->c_int,
 mpv_command_async:unsafe extern "C" fn(Ptr,u64,*const *const c_char)->c_int,
 mpv_observe_property:unsafe extern "C" fn(Ptr,u64,*const c_char,c_int)->c_int,
 mpv_wait_event:unsafe extern "C" fn(Ptr,f64)->*const Event,
 mpv_terminate_destroy:unsafe extern "C" fn(Ptr),
 mpv_render_context_create:unsafe extern "C" fn(*mut Ptr,Ptr,*mut Param)->c_int,
 mpv_render_context_render:unsafe extern "C" fn(Ptr,*mut Param)->c_int,
 mpv_render_context_free:unsafe extern "C" fn(Ptr),
 mpv_render_context_set_update_callback:unsafe extern "C" fn(Ptr,Option<unsafe extern "C" fn(Ptr)>,Ptr),
}
#[link(name = "EGL")]
extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> Ptr;
}
#[link(name = "GL")]
extern "C" {
    fn glGetIntegerv(name: u32, value: *mut c_int);
    fn glGetString(name: u32) -> *const c_char;
    fn glBindFramebuffer(target: u32, id: u32);
    fn glReadPixels(x: c_int, y: c_int, w: c_int, h: c_int, format: u32, kind: u32, data: Ptr);
    fn glGetError() -> u32;
}
unsafe extern "C" fn get_proc(_: Ptr, name: *const c_char) -> Ptr {
    eglGetProcAddress(name)
}
unsafe extern "C" fn redraw(data: Ptr) {
    (*(data as *const AtomicBool)).store(true, Ordering::Release);
}
struct Player {
    api: Api,
    handle: Ptr,
    context: Cell<Ptr>,
    dirty: AtomicBool,
    status: RefCell<Value>,
    id: String,
    _temporary: Option<tempfile::NamedTempFile>,
    probe: bool,
    frames: Cell<u64>,
    crop: Cell<bool>,
    audio_only: Cell<bool>,
    repeat: bool,
}
impl Player {
    fn check(code: c_int) -> Result<(), String> {
        if code < 0 {
            Err(format!("libmpv operation failed ({code})"))
        } else {
            Ok(())
        }
    }
    fn fail(&self, message: &str) {
        let mut status = self.status.borrow_mut();
        if status["error"].as_str() != Some(message) {
            super::notice(&self.id, "playback", true);
            status["error"] = json!(message);
        }
    }
    fn command(&self, args: &[&str]) -> Result<(), String> {
        let strings: Result<Vec<_>, _> = args.iter().map(|s| CString::new(*s)).collect();
        let strings = strings.map_err(|_| "Invalid video argument".to_string())?;
        let mut pointers: Vec<_> = strings.iter().map(|s| s.as_ptr()).collect();
        pointers.push(std::ptr::null());
        Self::check(unsafe { (self.api.mpv_command_async)(self.handle, 0, pointers.as_ptr()) })
    }
    fn clear_crop(&self) -> Result<(), String> {
        if self.crop.get() {
            self.command(&["set", "file-local-options/end", "none"])?;
            self.command(&[
                "set",
                "file-local-options/loop-file",
                if self.repeat { "inf" } else { "no" },
            ])?;
            self.crop.set(false);
        }
        Ok(())
    }
    fn poll(&self) {
        for _ in 0..100 {
            unsafe {
                let event = &*(self.api.mpv_wait_event)(self.handle, 0.0);
                if event.id == 0 {
                    break;
                }
                if event.error < 0 {
                    self.fail("Video decode or control failed; see Logs");
                }
                if event.id == 7 && !event.data.is_null() {
                    // mpv_event_end_file starts with reason and error.
                    let end = event.data as *const c_int;
                    if *end == 4 {
                        self.fail("Cannot decode this video");
                    }
                }
                if event.id == 22 && !event.data.is_null() {
                    let p = &*(event.data as *const Property);
                    if p.data.is_null() {
                        continue;
                    }
                    let key = match CStr::from_ptr(p.name).to_bytes() {
                        b"time-pos" => "position",
                        b"duration" => "duration",
                        b"width" => "width",
                        b"height" => "height",
                        b"pause" => "paused",
                        b"mute" => "muted",
                        b"volume" => "volume",
                        b"eof-reached" => "ended",
                        _ => continue,
                    };
                    let value = match p.format {
                        3 => json!(*(p.data as *const c_int) != 0),
                        4 => json!(*(p.data as *const i64)),
                        5 => json!(*(p.data as *const f64)),
                        _ => continue,
                    };
                    self.status.borrow_mut()[key] = value;
                }
            }
        }
    }
    fn free_render(&self, area: &gtk::GLArea) {
        let ctx = self.context.replace(std::ptr::null_mut());
        if !ctx.is_null() {
            area.make_current();
            unsafe {
                (self.api.mpv_render_context_set_update_callback)(ctx, None, std::ptr::null_mut());
                (self.api.mpv_render_context_free)(ctx);
            }
        }
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            (self.api.mpv_terminate_destroy)(self.handle);
        }
        super::notice(&self.id, "destroy", false);
    }
}
struct Surface {
    area: gtk::GLArea,
    player: Rc<Player>,
    timer: Option<gtk::glib::SourceId>,
}
impl Drop for Surface {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            if let Some(source) = gtk::glib::MainContext::default().find_source_by_id(&timer) {
                source.destroy();
            }
        }
        self.player.free_render(&self.area);
        unsafe {
            self.area.destroy();
        }
    }
}
#[derive(Default)]
struct State {
    fixed: Option<gtk::Fixed>,
    players: HashMap<String, Surface>,
}
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
pub fn install(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window missing")?;
    window
        .with_webview(|platform| {
            let web = platform.inner();
            let Some(parent) = web.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) else {
                super::notice("shell", "install", true);
                return;
            };
            let overlay = gtk::Overlay::new();
            let fixed = gtk::Fixed::new();
            parent.remove(&web);
            overlay.add(&web);
            overlay.add_overlay(&fixed);
            overlay.set_overlay_pass_through(&fixed, true);
            // GTK pass-through forwards clicks, but this full-window layer still blocks drag targets.
            let restore = Rc::new(Cell::new(false));
            let layer = fixed.clone();
            let visible = restore.clone();
            web.connect_drag_begin(move |_, _| {
                visible.set(layer.is_visible());
                layer.hide();
            });
            let layer = fixed.clone();
            web.connect_drag_end(move |_, _| {
                if restore.replace(false) {
                    layer.show();
                }
            });
            overlay.set_hexpand(true);
            overlay.set_vexpand(true);
            parent.pack_start(&overlay, true, true, 0);
            overlay.show_all();
            STATE.with(|s| s.borrow_mut().fixed = Some(fixed));
        })
        .map_err(|e| e.to_string())
}
pub fn open(
    id: String,
    path: PathBuf,
    temporary: Option<tempfile::NamedTempFile>,
    auto_play: bool,
    repeat: bool,
    muted: bool,
) -> Result<(), String> {
    STATE.with(|s|{
 let mut state=s.borrow_mut();
 let fixed=state.fixed.as_ref().ok_or("Native video surface is unavailable")?;
 if state.players.len()>=8 || state.players.contains_key(&id){return Err("Too many video previews".into());}
 unsafe{libc::setlocale(libc::LC_NUMERIC,c"C".as_ptr());}
 let api=unsafe{Api::load()?};let handle=unsafe{(api.mpv_create)()};if handle.is_null(){return Err("Cannot create video player".into());}
 let p=Rc::new(Player{api,handle,context:Cell::new(std::ptr::null_mut()),dirty:AtomicBool::new(true),status:RefCell::new(json!({"position":0,"duration":0,"width":0,"height":0,"paused":!auto_play,"muted":muted,"volume":100,"ended":false})),id:id.clone(),_temporary:temporary,probe:std::env::var_os("DEAS_VIDEO_PROBE").is_some(),frames:Cell::new(0),crop:Cell::new(false),audio_only:Cell::new(false),repeat});
 // shortcut: software decode until hardware decode is verified on supported native displays.
 for (key,value) in [("config","no"),("load-scripts","no"),("ytdl","no"),("vo","libmpv"),("hwdec","no"),("video-timing-offset","0"),("idle","yes"),("keep-open","yes"),("input-terminal","no"),("osc","no"),("audio-display","no"),("demuxer-lavf-o","protocol_whitelist=file"),("mute",if muted{"yes"}else{"no"}),("pause",if auto_play{"no"}else{"yes"}),("loop-file",if repeat{"inf"}else{"no"})] {
 let key=CString::new(key).unwrap();let value=CString::new(value).unwrap();Player::check(unsafe{(p.api.mpv_set_option_string)(handle,key.as_ptr(),value.as_ptr())}).map_err(|e|format!("{e}: {}",key.to_string_lossy()))?;
 }
 Player::check(unsafe{(p.api.mpv_initialize)(handle)})?;
 for (key,format) in [("time-pos",5),("duration",5),("width",4),("height",4),("pause",3),("mute",3),("volume",5),("eof-reached",3)] {let key=CString::new(key).unwrap();Player::check(unsafe{(p.api.mpv_observe_property)(handle,0,key.as_ptr(),format)})?;}
 let area=gtk::GLArea::new();area.set_required_version(3,2);area.set_auto_render(false);area.set_size_request(1,1);area.set_no_show_all(true);
 let player=p.clone();area.connect_realize(move |area| {
 area.make_current();
 let result=(|| {
 if let Some(e)=area.error(){return Err(e.to_string());}
 if player.probe {unsafe{eprintln!("DEAS_VIDEO_GL {}",CStr::from_ptr(glGetString(0x1f01)).to_string_lossy());}}
 let mut init=Init{get:get_proc,ctx:std::ptr::null_mut()};
 let mut params=[Param{kind:1,data:c"opengl".as_ptr() as Ptr},Param{kind:2,data:&mut init as *mut _ as Ptr},Param{kind:0,data:std::ptr::null_mut()}];
 let mut ctx=std::ptr::null_mut();Player::check(unsafe{(player.api.mpv_render_context_create)(&mut ctx,player.handle,params.as_mut_ptr())})?;player.context.set(ctx);
 unsafe{(player.api.mpv_render_context_set_update_callback)(ctx,Some(redraw),&player.dirty as *const _ as Ptr);}
 player.command(&["loadfile",path.to_str().ok_or("Video path is not UTF-8")?,"replace"])?;Ok::<(),String>(())
 })();if let Err(error)=result{player.fail(&error);}
 });
 let player=p.clone();area.connect_render(move |area,_|{
 let ctx=player.context.get();if ctx.is_null(){return gtk::glib::Propagation::Stop;}
 let mut id=0;unsafe{glGetIntegerv(0x8ca6,&mut id);}
 let mut fbo=Fbo{id,width:area.allocated_width()*area.scale_factor(),height:area.allocated_height()*area.scale_factor(),format:0};let mut flip=1;
 let mut params=[Param{kind:3,data:&mut fbo as *mut _ as Ptr},Param{kind:4,data:&mut flip as *mut _ as Ptr},Param{kind:0,data:std::ptr::null_mut()}];
 if Player::check(unsafe{(player.api.mpv_render_context_render)(ctx,params.as_mut_ptr())}).is_err(){player.fail("Video rendering failed");}
 if player.probe {
 player.frames.set(player.frames.get()+1);
 if player.frames.get().is_multiple_of(30) && fbo.width>=32 && fbo.height>=24 {
 let mut pixels=[0u8;32*24*4];unsafe{glBindFramebuffer(0x8ca8,id as u32);glReadPixels(fbo.width/2-16,fbo.height/2-12,32,24,0x1908,0x1401,pixels.as_mut_ptr().cast());}
 let hash=pixels.iter().fold(2166136261u32,|h,b|(h^*b as u32).wrapping_mul(16777619));
 let range=pixels.iter().max().unwrap()-pixels.iter().min().unwrap();
 eprintln!("DEAS_VIDEO_FRAME id={} hash={hash} range={range} gl_error={} position={}",player.id,unsafe{glGetError()},player.status.borrow()["position"]);
 }
 }
 gtk::glib::Propagation::Stop
 });
 let player=p.clone();area.connect_unrealize(move |a|player.free_render(a));
 // Register handlers before insertion: a realized parent may realize children immediately.
 fixed.put(&area,0,0);
 let weak=area.downgrade();let player=p.clone();let timer=gtk::glib::timeout_add_local(std::time::Duration::from_millis(16),move ||{
 let Some(area)=weak.upgrade() else{return gtk::glib::ControlFlow::Break;};player.poll();if player.dirty.swap(false,Ordering::AcqRel){area.queue_render();}gtk::glib::ControlFlow::Continue
 });
 state.players.insert(id,Surface{area,player:p,timer:Some(timer)});Ok(())
 })
}
pub fn update(id: &str, action: Action) -> Result<Value, String> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        if matches!(action, Action::Close) {
            state.players.remove(id);
            return Ok(Value::Null);
        }
        let surface = state.players.get(id).ok_or("Video preview closed")?;
        let p = &surface.player;
        match action {
            Action::Bounds {
                x,
                y,
                width,
                height,
                visible,
                audio_only,
            } => {
                if p.audio_only.replace(audio_only) != audio_only {
                    // Hidden GLAreas cannot render; disable video so its output never stalls audio.
                    p.command(&["set", "vid", if audio_only { "no" } else { "auto" }])?;
                }
                if visible && !audio_only && width > 0 && height > 0 {
                    state.fixed.as_ref().unwrap().move_(&surface.area, x, y);
                    surface.area.set_size_request(width, height);
                    surface.area.show();
                } else {
                    surface.area.hide();
                    if !visible || width <= 0 || height <= 0 {
                        p.command(&["set", "pause", "yes"])?;
                    }
                }
            }
            Action::Pause { paused } => {
                if !paused {
                    p.clear_crop()?;
                }
                if !paused && p.status.borrow()["ended"] == true {
                    p.command(&["seek", "0", "absolute+exact"])?;
                }
                p.command(&["set", "pause", if paused { "yes" } else { "no" }])?;
            }
            Action::Seek { seconds } => {
                p.clear_crop()?;
                p.command(&["seek", &seconds.to_string(), "absolute+exact"])?
            }
            Action::Crop { start, end } => {
                p.crop.set(true);
                p.command(&["set", "pause", "yes"])?;
                // mpv clips video frames and audio samples at this file-local end, not a UI timer.
                p.command(&["set", "file-local-options/end", &end.to_string()])?;
                p.command(&["set", "file-local-options/loop-file", "no"])?;
                p.command(&["seek", &start.to_string(), "absolute+exact"])?;
                p.command(&["set", "pause", "no"])?;
            }
            Action::Mute { muted } => {
                p.command(&["set", "mute", if muted { "yes" } else { "no" }])?
            }
            Action::Volume { volume } => p.command(&["set", "volume", &volume.to_string()])?,
            Action::Status => {
                p.poll();
                return Ok(p.status.borrow().clone());
            }
            Action::Close => unreachable!(),
        };
        Ok(Value::Null)
    })
}
pub fn shutdown() {
    STATE.with(|s| s.borrow_mut().players.clear());
}
