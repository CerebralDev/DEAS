use super::*;
use crate::install::{Options, Request};
pub(crate) const SERVER: &str = r#"
import json,sys,http.server,socketserver,time,os
args=sys.argv[1:]
assert '--disable-auto-launch' in args
assert args[args.index('--enable-cors-header')+1]=='tauri://localhost'
assert args[args.index('--listen')+1]=='127.0.0.1'
assert '--cpu' in args
assert os.environ.get('PYTHONPATH') is None
port=int(args[args.index('--port')+1])
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  with open('requests.log','a') as f: f.write(self.path+'\n')
  body=json.dumps({'FixtureNode':{}} if self.path=='/object_info' else {'devices':[{'name':'CPU fixture'}]}).encode()
  self.send_response(200); self.send_header('Content-Length',str(len(body))); self.end_headers(); self.wfile.write(body)
print('fixture starting',flush=True)
with socketserver.TCPServer(('127.0.0.1',port),Handler) as server: server.serve_forever()
"#;
pub(crate) fn fixture(script: &str) -> (tempfile::TempDir, Approved) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for name in [
        "runtime/python",
        "ComfyUI",
        "home",
        "cache",
        "config",
        "tmp",
    ] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    let result = Command::new("/usr/bin/python3")
        .args(["-m", "venv", "--without-pip", "--copies"])
        .arg(root.join("venv"))
        .status()
        .unwrap();
    assert!(result.success());
    fs::write(root.join("ComfyUI/main.py"), script).unwrap();
    let plan = Plan {
        gguf_ref: None,
        preview_ref: None,
        reopen_id: None,
        id: uuid::Uuid::new_v4().to_string(),
        request: Request {
            storage_path: root.to_str().unwrap().into(),
            backend: Backend::Cpu,
            features: vec![],
            options: Options::default(),
            comfy_ref: "master".into(),
            folders: None,
            takeover: false,
        },
        steps: vec![],
        warnings: vec![],
        unsupported_options: vec![],
        can_execute: true,
        hardware: None,
        torch: Some(crate::install::torch_profile::cpu()),
    };
    fs::write(
        root.join("deas-plan.json"),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    fs::write(root.join("deas-install.json"), serde_json::to_vec(&serde_json::json!({"planId":plan.id,"backend":"cpu","hardware":null,"torchProfile":plan.torch,"python":root.join("venv/bin/python"),"generationReady":false})).unwrap()).unwrap();
    let approved = Approved::capture(&plan).unwrap();
    (tmp, approved)
}
pub(crate) fn await_state(lifecycle: &Lifecycle, state: &str) -> Status {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let current = lifecycle.status();
        if current.state == state {
            return current;
        }
        assert!(
            Instant::now() < deadline,
            "waiting for {state}: {current:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
#[test]
fn running_interrupt_adapter_auth_and_mutex_race_fixture() {
    let result = Command::new("/usr/bin/python3")
        .args([
            "-c",
            include_str!("running_interrupt_tests.py"),
            include_str!("running_interrupt.py"),
        ])
        .env_remove("DEAS_TEST_COMFY_SOURCE")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn running_interrupt_protocol_accepts_current_core_and_rejects_moved_lock() {
    // Statement checks, not hashes: the live `master` checkout (when present) must pass.
    let live = PathBuf::from("/data/GitHub/deas-environment/ComfyUI");
    if live.is_dir() {
        for name in INTERRUPT_FILES {
            let text = fs::read_to_string(live.join(name)).unwrap();
            assert!(interrupt_protocol(name, &text), "{name} protocol rejected");
        }
    }
    let ok = "class PromptQueue:\n    def __init__(self):\n        self.mutex = threading.RLock()\n        self.not_empty = threading.Condition(self.mutex)\n\n    def get(self, timeout=None):\n        with self.not_empty:\n            pass\n\n    def task_done(self, item_id):\n        with self.mutex:\n            pass\n\n    def get_current_queue(self):\n        with self.mutex:\n            pass\n\nclass PromptExecutor:\n    async def execute_async(self, prompt):\n        nodes.interrupt_processing(False)\n";
    assert!(interrupt_protocol("execution.py", ok));
    // task_done releasing the queue lock before its body could interrupt a foreign successor.
    assert!(!interrupt_protocol(
        "execution.py",
        &ok.replace(
            "def task_done(self, item_id):\n        with self.mutex:",
            "def task_done(self, item_id):\n        with self.other:"
        )
    ));
    assert!(!interrupt_protocol(
        "execution.py",
        &ok.replace("nodes.interrupt_processing(False)", "pass")
    ));
    assert!(!interrupt_protocol(
        "server.py",
        "# unsupported legacy handler"
    ));
}

#[test]
fn running_interrupt_unsupported_source_cannot_gain_authority_after_launch() {
    let (tmp, approved) = fixture(SERVER);
    let lifecycle = Lifecycle::default();
    lifecycle.launch(approved, Duration::from_secs(5)).unwrap();
    let base = reqwest::Url::parse(&await_state(&lifecycle, "running").endpoint.unwrap()).unwrap();
    let owner = lifecycle.interruption_owner();
    assert!(owner(&base).is_err());
    fs::write(
        tmp.path().join("ComfyUI/server.py"),
        "# unsupported legacy handler",
    )
    .unwrap();
    assert!(owner(&base).is_err());
    assert!(locked(&lifecycle.inner).interrupt_source.is_none());
    lifecycle.stop();
    assert!(owner(&base).is_err());
}

#[test]
#[ignore = "explicit DEAS_TEST_COMFY_SOURCE directory: read-only exact pinned source, no model imports"]
fn running_interrupt_source_backed_launch_proof_and_queue_race() {
    let source = PathBuf::from(
        std::env::var_os("DEAS_TEST_COMFY_SOURCE").expect("set DEAS_TEST_COMFY_SOURCE"),
    );
    for crlf in [false, true] {
        let (tmp, approved) = fixture(SERVER);
        let root = tmp.path();
        for name in INTERRUPT_FILES {
            let bytes = fs::read(source.join(name)).unwrap();
            let bytes = if crlf {
                String::from_utf8(bytes)
                    .unwrap()
                    .replace("\r\n", "\n")
                    .replace('\n', "\r\n")
                    .into_bytes()
            } else {
                bytes
            };
            fs::write(root.join("ComfyUI").join(name), bytes).unwrap();
        }
        InterruptSource::capture(root).unwrap().validate().unwrap();
        // Fixture interpreter serves only bounded CPU HTTP; pinned main.py is never imported/executed.
        fs::write(
            root.join(VENV_PYTHON),
            format!(
                "#!/usr/bin/python3\n{}",
                SERVER.replace(
                    "args=sys.argv[1:]",
                    "assert os.environ.get('DEAS_RUNNING_INTERRUPT_TOKEN')\nargs=sys.argv[1:]"
                )
            ),
        )
        .unwrap();
        let approved = Approved::capture(approved.plan()).unwrap();
        let lifecycle = Lifecycle::default();
        lifecycle.launch(approved, Duration::from_secs(5)).unwrap();
        let endpoint = await_state(&lifecycle, "running").endpoint.unwrap();
        let base = reqwest::Url::parse(&endpoint).unwrap();
        let owner = lifecycle.interruption_owner();
        let (epoch, token) = owner(&base).unwrap();
        assert!(!token.is_empty());
        assert_eq!(owner(&base).unwrap(), (epoch, token.clone()));
        assert!(owner(&reqwest::Url::parse(&format!("{endpoint}foreign/")).unwrap()).is_err());
        let adapter = root.join("ComfyUI/custom_nodes/deas_running_interrupt.py");
        assert_eq!(
            fs::read(&adapter).unwrap(),
            include_bytes!("running_interrupt.py")
        );
        fs::write(&adapter, "# changed").unwrap();
        assert!(owner(&base).is_err());
        fs::write(&adapter, include_bytes!("running_interrupt.py")).unwrap();
        assert!(owner(&base).is_ok());
        let core = root.join("ComfyUI/server.py");
        let original = fs::read(&core).unwrap();
        fs::write(&core, b"# changed after launch").unwrap();
        assert!(owner(&base).is_err());
        fs::write(&core, original).unwrap();
        assert!(owner(&base).is_ok());
        lifecycle.stop();
        assert!(owner(&base).is_err());
        assert!(!crate::logs::native_logs(0)
            .iter()
            .any(|e| e.text.contains(&token)));
    }
    let result = Command::new("/usr/bin/python3")
        .args([
            "-c",
            include_str!("running_interrupt_tests.py"),
            include_str!("running_interrupt.py"),
        ])
        .env("DEAS_TEST_COMFY_SOURCE", source)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn output_naming_python_contract_fixture() {
    let result = Command::new("/usr/bin/python3")
        .args([
            "-c",
            include_str!("output_naming_tests.py"),
            include_str!("output_naming.py"),
        ])
        .env_remove("PYTHONPATH")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn output_naming_adapter_is_installed_and_refuses_symlinks() {
    let (tmp, _) = fixture(SERVER);
    install_output_naming(tmp.path()).unwrap();
    let plugin = tmp
        .path()
        .join("ComfyUI/custom_nodes/deas_output_naming.py");
    assert_eq!(
        fs::read_to_string(&plugin).unwrap(),
        include_str!("output_naming.py")
    );
    install_output_naming(tmp.path()).unwrap();
    fs::remove_file(&plugin).unwrap();
    let foreign = tmp.path().join("foreign.py");
    fs::write(&foreign, "untouched").unwrap();
    std::os::unix::fs::symlink(&foreign, &plugin).unwrap();
    assert!(install_output_naming(tmp.path()).is_err());
    assert_eq!(fs::read_to_string(foreign).unwrap(), "untouched");
}

#[test]
fn image_prompting_python_contract_fixture() {
    let result = Command::new("/usr/bin/python3")
        .args([
            "-c",
            include_str!("image_prompting_tests.py"),
            include_str!("image_prompting.py"),
        ])
        .env_remove("PYTHONPATH")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn image_postprocess_python_contract_fixture() {
    let script = r#"
import sys
scope = {}
exec(sys.argv[1], scope)
Gate = scope['NODE_CLASS_MAPPINGS']['DEAS_ImagePostprocessGate']
assert scope['NODE_CLASS_MAPPINGS']['DEAS_VideoPostprocessGate'] is Gate
gate = Gate()
assert Gate.INPUT_TYPES() == {'required': {'baseline': ('IMAGE,STRING',), 'image': ('IMAGE', {'lazy': True})}, 'optional': {'audio': ('AUDIO', {'lazy': True})}}
assert Gate.RETURN_TYPES == ('IMAGE', 'AUDIO')
image, audio = object(), object()
for baseline in [object(), 'saved.png']:
    # Image graphs: audio unconnected (absent kwarg) is never requested.
    assert gate.check_lazy_status(baseline) == ['image']
    assert gate.check_lazy_status(baseline, image) == []
    assert gate.forward(baseline, image) == (image, None)
    # Looped video: connected audio arrives as None until evaluated.
    assert gate.check_lazy_status(baseline, None, None) == ['image', 'audio']
    assert gate.check_lazy_status(baseline, image, None) == ['audio']
    assert gate.check_lazy_status(baseline, image, audio) == []
    assert gate.forward(baseline, image, audio) == (image, audio)
"#;
    let result = Command::new("/usr/bin/python3")
        .args(["-c", script, include_str!("image_postprocess.py")])
        .env_remove("PYTHONPATH")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn image_prompting_installed_before_owned_child_start() {
    let script = SERVER.replace(
        "args=sys.argv[1:]",
        "assert os.path.isfile('custom_nodes/deas_image_prompting.py')\nargs=sys.argv[1:]",
    );
    let (tmp, approved) = fixture(&script);
    let lifecycle = Lifecycle::default();
    lifecycle
        .launch(approved.clone(), Duration::from_secs(5))
        .unwrap();
    await_state(&lifecycle, "running");
    let directory = tmp.path().join("ComfyUI/custom_nodes");
    assert_eq!(
        fs::read(directory.join("deas_image_prompting.py")).unwrap(),
        include_bytes!("image_prompting.py")
    );
    assert_eq!(
        fs::read(directory.join("deas_image_postprocess.py")).unwrap(),
        include_bytes!("image_postprocess.py")
    );
    let mut entries: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        [
            "deas_image_postprocess.py",
            "deas_image_prompting.py",
            "deas_output_naming.py"
        ]
    );
    lifecycle.stop();
}

#[test]
fn image_prompting_install_rejects_linked_files_and_directories() {
    let (tmp, approved) = fixture(SERVER);
    let directory = tmp.path().join("ComfyUI/custom_nodes");
    fs::create_dir(&directory).unwrap();
    let foreign = tmp.path().join("foreign.py");
    fs::write(&foreign, "untouched").unwrap();
    let plugin = directory.join("deas_image_prompting.py");
    std::os::unix::fs::symlink(&foreign, &plugin).unwrap();
    let lifecycle = Lifecycle::default();
    assert!(lifecycle
        .launch(approved.clone(), Duration::from_secs(1))
        .is_err());
    assert!(locked(&lifecycle.inner).child.is_none());
    assert_eq!(fs::read_to_string(&foreign).unwrap(), "untouched");
    fs::remove_file(plugin).unwrap();
    fs::remove_file(directory.join("deas_output_naming.py")).unwrap();
    fs::remove_dir(&directory).unwrap();
    let foreign_directory = tmp.path().join("foreign_nodes");
    fs::create_dir(&foreign_directory).unwrap();
    std::os::unix::fs::symlink(&foreign_directory, &directory).unwrap();
    assert!(lifecycle.launch(approved, Duration::from_secs(1)).is_err());
    assert!(locked(&lifecycle.inner).child.is_none());
    assert_eq!(fs::read_dir(foreign_directory).unwrap().count(), 0);
}

#[test]
fn a_second_lifecycle_cannot_start_the_same_environment_until_the_first_stops() {
    let (_tmp, approved) = fixture(SERVER);
    let first = Lifecycle::default();
    let second = Lifecycle::default();
    first
        .launch(approved.clone(), Duration::from_secs(5))
        .unwrap();
    await_state(&first, "running");
    let duplicate = second.launch(approved.clone(), Duration::from_secs(5));
    first.stop();
    assert!(
        duplicate.is_err(),
        "second instance must not spawn a duplicate backend"
    );
    second.launch(approved, Duration::from_secs(5)).unwrap();
    await_state(&second, "running");
    second.stop();
}
#[test]
fn real_owned_child_requires_api_before_running_and_stop_is_exact() {
    let (tmp, approved) = fixture(SERVER);
    let lifecycle = Lifecycle::default();
    let cancellation_owner = lifecycle.cancellation_owner();
    assert!(cancellation_owner(&reqwest::Url::parse("http://127.0.0.1:1/").unwrap()).is_err());
    let initial = lifecycle
        .launch(approved.clone(), Duration::from_secs(5))
        .unwrap();
    assert_eq!(initial.state, "starting");
    assert!(initial.endpoint.is_none());
    assert_eq!(initial.output_root, Some(tmp.path().join("ComfyUI/output")));
    assert_eq!(initial.model_roots, vec![tmp.path().join("ComfyUI/models")]);
    assert!(lifecycle.launch(approved, Duration::from_secs(5)).is_err());
    let status = await_state(&lifecycle, "running");
    assert!(!status.generation_ready);
    let endpoint = status.endpoint.unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let base = reqwest::Url::parse(&endpoint).unwrap();
    let epoch = cancellation_owner(&base).unwrap();
    assert_eq!(cancellation_owner(&base).unwrap(), epoch);
    for suffix in ["other/", "?token=private", "#fragment"] {
        assert!(
            cancellation_owner(&reqwest::Url::parse(&format!("{endpoint}{suffix}")).unwrap())
                .is_err()
        );
    }
    assert!(cancellation_owner(
        &reqwest::Url::parse(&endpoint.replace("127.0.0.1", "localhost")).unwrap()
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(tmp.path().join("ComfyUI/requests.log")).unwrap(),
        "/object_info\n/system_stats\n"
    );
    let pid = locked(&lifecycle.inner).child.as_ref().unwrap().id();
    assert_eq!(lifecycle.stop().state, "stopped");
    assert!(cancellation_owner(&base).is_err());
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(lifecycle.status().endpoint.is_none());
    assert!(lifecycle.status().output_root.is_none());
    assert!(lifecycle.status().model_roots.is_empty());
    assert!(!lifecycle.stop().generation_ready);
}
#[test]
fn readiness_timeout_kills_only_owned_process() {
    let (_tmp, approved) = fixture("import time; print('not an API',flush=True); time.sleep(60)");
    let lifecycle = Lifecycle::default();
    lifecycle
        .launch(approved, Duration::from_millis(300))
        .unwrap();
    let pid = locked(&lifecycle.inner).child.as_ref().unwrap().id();
    let status = await_state(&lifecycle, "failed");
    assert!(status.error.unwrap().contains("timed out"));
    assert!(status.endpoint.is_none());
    assert!(!status.generation_ready);
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(crate::logs::native_logs(0)
        .iter()
        .any(|entry| entry.text.contains("not an API")));
}
#[test]
fn startup_failure_retains_real_logs() {
    let (_tmp, approved) =
        fixture("import sys; print('fixture startup failed',flush=True); sys.exit(7)");
    let lifecycle = Lifecycle::default();
    lifecycle.launch(approved, Duration::from_secs(2)).unwrap();
    let status = await_state(&lifecycle, "failed");
    assert!(status.error.unwrap().contains("exited"));
    assert!(!status.generation_ready);
    let deadline = Instant::now() + Duration::from_secs(1);
    while !crate::logs::native_logs(0)
        .iter()
        .any(|entry| entry.text.contains("fixture startup failed"))
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn manifest_tamper_and_symlink_replacement_never_spawn() {
    let (tmp, approved) = fixture(SERVER);
    fs::write(tmp.path().join("deas-install.json"), "{}").unwrap();
    let lifecycle = Lifecycle::default();
    assert!(lifecycle
        .launch(approved, Duration::from_secs(1))
        .unwrap_err()
        .contains("manifest"));
    assert!(locked(&lifecycle.inner).child.is_none());
    let (tmp, approved) = fixture(SERVER);
    let main = tmp.path().join("ComfyUI/main.py");
    fs::rename(&main, tmp.path().join("original.py")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("original.py"), &main).unwrap();
    assert!(lifecycle
        .launch(approved, Duration::from_secs(1))
        .unwrap_err()
        .contains("symlink"));
}
#[test]
fn drop_cleans_owned_group() {
    let (_tmp, approved) = fixture("import time; time.sleep(60)");
    let lifecycle = Lifecycle::default();
    lifecycle.launch(approved, Duration::from_secs(5)).unwrap();
    let pid = locked(&lifecycle.inner).child.as_ref().unwrap().id();
    drop(lifecycle);
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
}
#[test]
fn changed_launch_file_never_spawns() {
    let (tmp, approved) = fixture(SERVER);
    fs::write(tmp.path().join("ComfyUI/main.py"), "print('replaced')").unwrap();
    let lifecycle = Lifecycle::default();
    assert!(lifecycle.launch(approved, Duration::from_secs(1)).is_err());
    assert!(locked(&lifecycle.inner).child.is_none());
}
#[test]
fn stalled_owned_http_respects_overall_readiness_deadline() {
    let (tmp, approved) = fixture(&SERVER.replace(
        "  body=json.dumps",
        "  with open('stalled-request.pid','w') as f: f.write(str(os.getpid()))\n  time.sleep(60)\n  body=json.dumps",
    ));
    let lifecycle = Lifecycle::default();
    let overall_deadline = Duration::from_millis(300);
    let total_started = Instant::now();
    let launch_started = Instant::now();
    lifecycle.launch(approved, overall_deadline).unwrap();
    let launch_duration = launch_started.elapsed();
    let terminal_started = Instant::now();
    let status = await_state(&lifecycle, "failed");
    let terminal_observation = terminal_started.elapsed();
    let total_elapsed = total_started.elapsed();
    let stalled_pid = fs::read_to_string(tmp.path().join("ComfyUI/stalled-request.pid"))
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok());
    let timing = format!(
        "caller-observed durations: launch_call={launch_duration:?}, post_launch_terminal_state={terminal_observation:?}, total={total_elapsed:?}"
    );
    eprintln!(
        "stalled readiness diagnostics: {timing}; stalled_request_pid={stalled_pid:?}; terminal_status={status:?}"
    );

    assert!(
        status
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Overall readiness deadline reached")),
        "{timing}; {status:?}"
    );
    assert!(!status.generation_ready, "{timing}; {status:?}");
    assert!(status.endpoint.is_none(), "{timing}; {status:?}");
    assert!(locked(&lifecycle.inner).child.is_none(), "{timing}");
    assert!(
        stalled_pid.is_some(),
        "fixture never recorded entry into the stalled HTTP request; {timing}; {status:?}"
    );
    if let Some(pid) = stalled_pid {
        assert!(!Path::new(&format!("/proc/{pid}")).exists(), "{timing}");
    }
    // Bound caller-observed post-launch liveness, excluding synchronous launch
    // validation/spawn. The supervisor's 300 ms clock starts after runtime
    // construction and may already be running when launch returns; this is not
    // a precise supervisor/cleanup measurement. Retain launch/total diagnostics.
    assert!(
        terminal_observation < Duration::from_secs(1),
        "{timing}; {status:?}"
    );
}
#[test]
fn running_child_losing_owned_listener_is_failed_not_adopted() {
    let script = SERVER.replace(
        "server.serve_forever()",
        "server.handle_request(); server.handle_request(); time.sleep(0.3)\ntime.sleep(60)",
    );
    let (_tmp, approved) = fixture(&script);
    let lifecycle = Lifecycle::default();
    lifecycle.launch(approved, Duration::from_secs(2)).unwrap();
    assert_eq!(await_state(&lifecycle, "running").state, "running");
    // The fixture closes its listener after both probes but stays alive.
    let status = await_state(&lifecycle, "failed");
    assert!(status.endpoint.is_none());
    assert!(!status.generation_ready);
}
#[test]
fn invalid_owned_api_never_marks_running() {
    let script = SERVER.replace("{'FixtureNode':{}}", "[]").replace(
        "  with open('requests.log','a')",
        "  if self.path == '/object_info' and os.path.exists('requests.log'): time.sleep(60)\n  with open('requests.log','a')",
    );
    let (_tmp, approved) = fixture(&script);
    let lifecycle = Lifecycle::default();
    // Preserve the malformed-API diagnostic even if a later probe stalls at the deadline.
    lifecycle.launch(approved, Duration::from_secs(2)).unwrap();
    let status = await_state(&lifecycle, "failed");
    assert!(
        status
            .error
            .as_ref()
            .is_some_and(|e| e.contains("object_info")),
        "{status:?}"
    );
    assert!(status.endpoint.is_none());
    assert!(!status.generation_ready);
}
#[test]
fn stop_kills_owned_descendant_but_not_unrelated_child() {
    use std::os::unix::process::CommandExt;
    struct Foreign(Child);
    impl Drop for Foreign {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut foreign_command = Command::new("/usr/bin/sleep");
    foreign_command.arg("30").process_group(0);
    let mut foreign = Foreign(foreign_command.spawn().unwrap());
    let script = "import subprocess,time\np=subprocess.Popen(['/usr/bin/sleep','30'])\nopen('descendant.pid','w').write(str(p.pid))\ntime.sleep(30)";
    let (tmp, approved) = fixture(script);
    let lifecycle = Lifecycle::default();
    lifecycle.launch(approved, Duration::from_secs(5)).unwrap();
    let pid_file = tmp.path().join("ComfyUI/descendant.pid");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !pid_file.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let pid: u32 = fs::read_to_string(pid_file).unwrap().parse().unwrap();
    lifecycle.stop();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Err(_) => break,
            Ok(stat) if stat.split_whitespace().nth(2) == Some("Z") => break,
            _ => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    assert!(foreign.0.try_wait().unwrap().is_none());
}
#[test]
fn stop_during_probe_does_not_allow_stale_running_status() {
    let (_tmp, approved) =
        fixture(&SERVER.replace("  body=json.dumps", "  time.sleep(60)\n  body=json.dumps"));
    let lifecycle = Lifecycle::default();
    lifecycle
        .launch(approved, Duration::from_millis(300))
        .unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(lifecycle.stop().state, "stopped");
    std::thread::sleep(Duration::from_millis(250));
    let status = lifecycle.status();
    assert_eq!(status.state, "stopped");
    assert!(status.endpoint.is_none());
}
#[test]
fn exact_ipc_status_schema_never_claims_generation() {
    assert_eq!(
        serde_json::to_value(Status::default()).unwrap(),
        serde_json::json!({"state":"stopped","endpoint":null,"error":null,"generationReady":false})
    );
}
#[test]
fn writable_or_replaced_owned_environment_is_rejected() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, approved) = fixture(SERVER);
    fs::set_permissions(
        tmp.path().join("ComfyUI"),
        fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    let lifecycle = Lifecycle::default();
    assert!(lifecycle
        .launch(approved, Duration::from_secs(1))
        .unwrap_err()
        .contains("writable"));
    assert!(locked(&lifecycle.inner).child.is_none());
    let (tmp, approved) = fixture(SERVER);
    fs::rename(
        tmp.path().join("ComfyUI"),
        tmp.path().join("original-comfy"),
    )
    .unwrap();
    fs::create_dir(tmp.path().join("ComfyUI")).unwrap();
    fs::copy(
        tmp.path().join("original-comfy/main.py"),
        tmp.path().join("ComfyUI/main.py"),
    )
    .unwrap();
    assert!(lifecycle
        .launch(approved, Duration::from_secs(1))
        .unwrap_err()
        .contains("identity"));
}
#[test]
fn foreign_listener_is_never_adopted_or_stopped() {
    let foreign = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = foreign.local_addr().unwrap().port();
    let (_tmp, approved) = fixture("import time; time.sleep(60)");
    let lifecycle = Lifecycle::default();
    lifecycle
        .launch_port(approved, Duration::from_secs(2), port)
        .unwrap();
    let status = await_state(&lifecycle, "failed");
    assert!(status.error.unwrap().contains("foreign listener"));
    assert!(status.endpoint.is_none());
    foreign.set_nonblocking(true).unwrap();
    assert_eq!(
        foreign.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "The foreign listener must not even receive readiness probes"
    );
    lifecycle.stop();
    assert!(std::net::TcpStream::connect(foreign.local_addr().unwrap()).is_ok());
}
#[test]
fn stop_allows_python_finally_and_drains_shutdown_output_before_restarting() {
    let script = SERVER.replace(
        "with socketserver.TCPServer(('127.0.0.1',port),Handler) as server: server.serve_forever()",
        "try:\n with socketserver.TCPServer(('127.0.0.1',port),Handler) as server: server.serve_forever()\nfinally:\n print('cleanup output\\n' * 20000, flush=True)\n time.sleep(0.1)\n open('clean-stop','w').write('finished')",
    );
    let (tmp, approved) = fixture(&script);
    let lifecycle = Lifecycle::default();
    for _ in 0..2 {
        lifecycle
            .launch(approved.clone(), Duration::from_secs(5))
            .unwrap();
        await_state(&lifecycle, "running");
        let before = crate::logs::native_logs(0).last().map_or(0, |e| e.seq);
        let started = Instant::now();
        assert_eq!(lifecycle.stop().state, "stopped");
        assert!(started.elapsed() < STOP_GRACE);
        let shutdown_logs = crate::logs::native_logs(before);
        assert!(shutdown_logs
            .iter()
            .any(|e| e.text == "Owned backend shut down gracefully"));
        assert!(!shutdown_logs.iter().any(|e| e.text.contains("escalating")));
        assert_eq!(
            fs::read_to_string(tmp.path().join("ComfyUI/clean-stop")).unwrap(),
            "finished"
        );
        fs::remove_file(tmp.path().join("ComfyUI/clean-stop")).unwrap();
    }
}

#[test]
fn stop_escalates_only_after_grace_when_descendant_ignores_interrupt() {
    let script = SERVER.replace(
        "args=sys.argv[1:]",
        "import subprocess\np=subprocess.Popen([sys.executable,'-c',\"import signal,time; signal.signal(signal.SIGINT,signal.SIG_IGN); open('stubborn-ready','w').write('ready'); time.sleep(60)\"])\nopen('descendant.pid','w').write(str(p.pid))\nargs=sys.argv[1:]",
    );
    let (tmp, approved) = fixture(&script);
    let lifecycle = Lifecycle::default();
    lifecycle.launch(approved, Duration::from_secs(5)).unwrap();
    await_state(&lifecycle, "running");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !tmp.path().join("ComfyUI/stubborn-ready").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let pid: u32 = fs::read_to_string(tmp.path().join("ComfyUI/descendant.pid"))
        .unwrap()
        .parse()
        .unwrap();
    let before = crate::logs::native_logs(0).last().map_or(0, |e| e.seq);
    let started = Instant::now();
    lifecycle.stop();
    assert!(started.elapsed() >= STOP_GRACE);
    assert!(started.elapsed() < STOP_GRACE + Duration::from_secs(2));
    let deadline = Instant::now() + Duration::from_secs(2);
    while fs::read_to_string(format!("/proc/{pid}/stat"))
        .is_ok_and(|s| s.split_whitespace().nth(2) != Some("Z"))
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(crate::logs::native_logs(before)
        .iter()
        .any(|e| e.text.contains("escalating to forced owned-group cleanup")));
}

#[test]
fn no_console_python_shutdown_pipe_wakes_asyncio_and_runs_finally() {
    use std::io::Write;
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("main.py"),
        r#"
import asyncio, sys
assert sys.argv == ['main.py', 'fixture-arg']
event_loop = asyncio.new_event_loop()
async def wait():
    open('ready', 'w').write('ready')
    await asyncio.Event().wait()
try:
    event_loop.run_until_complete(wait())
except KeyboardInterrupt:
    pass
finally:
    open('clean-stop', 'w').write('finished')
"#,
    )
    .unwrap();
    let mut child = Command::new("/usr/bin/python3")
        .args(["-u", "-c", WINDOWS_BOOTSTRAP, "main.py", "fixture-arg"])
        .current_dir(tmp.path())
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !tmp.path().join("ready").exists() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("bootstrap did not start");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.stdin.take().unwrap().write_all(b"stop\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("bootstrap did not stop");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(child.wait().unwrap().success());
    assert_eq!(
        fs::read_to_string(tmp.path().join("clean-stop")).unwrap(),
        "finished"
    );
}

#[test]
fn clean_log_line_strips_terminal_control_sequences() {
    assert_eq!(
        clean_log_line(b"\x1b[32m[INFO]\x1b[0m Starting server\n"),
        "[INFO] Starting server"
    );
    assert_eq!(clean_log_line(b" 10%\r 55%\r100% done\r\n"), "100% done");
    assert_eq!(
        clean_log_line(b"\x1b]0;title\x07a\tb\x08c\x1b]8;;x\x1b\\d"),
        "a\tbcd"
    );
    assert_eq!(
        clean_log_line("Gr\u{fc}\u{df}e \u{2713}\n".as_bytes()),
        "Grüße ✓"
    );
    assert_eq!(clean_log_line(b"\n"), "");
}
#[test]
fn log_capture_splits_progress_bar_redraws_into_live_segments() {
    let mut reader = std::io::Cursor::new(
        b" 25%|##  | 1/4\r 50%|### | 2/4\r100%|####| 4/4\r\nDone\nTail".to_vec(),
    );
    let mut segments = vec![];
    let mut line = Vec::new();
    loop {
        line.clear();
        match read_log_segment(&mut reader, &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = clean_log_line(&line);
                if !text.is_empty() {
                    segments.push(text);
                }
            }
        }
    }
    assert_eq!(
        segments,
        [
            " 25%|##  | 1/4",
            " 50%|### | 2/4",
            "100%|####| 4/4",
            "Done",
            "Tail"
        ]
    );
    // An endless line without delimiters is still bounded per segment.
    let mut long = std::io::Cursor::new(vec![b'x'; MAX_LOG_LINE as usize + 10]);
    let mut line = Vec::new();
    assert_eq!(
        read_log_segment(&mut long, &mut line).unwrap(),
        MAX_LOG_LINE as usize
    );
    line.clear();
    assert_eq!(read_log_segment(&mut long, &mut line).unwrap(), 10);
}
#[test]
fn native_backend_test_command_is_registered_and_permitted() {
    assert!(include_str!("../permissions/backend.toml").contains("\"native_backend_test\""));
    assert!(include_str!("main.rs").contains("deas::native_backend_test"));
}
#[test]
fn native_backend_test_lists_dependencies_of_the_approved_environment() {
    let (_tmp, approved) = fixture(SERVER);
    let engine = crate::install::Engine::default();
    let missing = engine.backend_test();
    assert_eq!(missing.len(), 5);
    assert!(missing.iter().all(|c| !c.ok));
    crate::install::lock(&engine.inner).approved = Some(approved);
    let checks = engine.backend_test();
    let ok = |id: &str| checks.iter().find(|c| c.id == id).unwrap().ok;
    assert!(ok("environment") && ok("python") && ok("comfyui"));
    assert!(!ok("nodes")); // The fixture environment has no DaSiWa node checkout.
    assert!(!ok("api"));
}
