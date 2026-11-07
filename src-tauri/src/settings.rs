//! Per-environment folder settings: where ComfyUI keeps models, input and output.
//! Stored as `deas-settings.json` beside the environment; the owned backend reads it at start.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE: &str = "deas-settings.json";
pub(crate) const MODEL_FOLDERS: [&str; 17] = [
    "checkpoints",
    "diffusion_models",
    "loras",
    "vae",
    "vae_approx",
    "text_encoders",
    "clip",
    "clip_vision",
    "controlnet",
    "upscale_models",
    "latent_upscale_models",
    "audio_encoders",
    "embeddings",
    "unet",
    "style_models",
    "gligen",
    "hypernetworks",
];

/// Disk folder names and backend namespaces are separate (SwarmUI uses two aliases).
pub(crate) fn model_folder_category(name: &str) -> Option<&'static str> {
    match name {
        "Stable-Diffusion" => Some("checkpoints"),
        "Lora" => Some("loras"),
        _ => MODEL_FOLDERS.iter().copied().find(|folder| *folder == name),
    }
}

/// Every configured root contains category folders; aliases share backend namespaces.
pub(crate) fn model_locations(root: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut locations: Vec<_> = MODEL_FOLDERS
        .iter()
        .map(|&category| (category, root.join(category)))
        .collect();
    for alias in ["Stable-Diffusion", "Lora"] {
        let path = root.join(alias);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir() || m.file_type().is_symlink())
        {
            locations.push((model_folder_category(alias).unwrap(), path));
        }
    }
    locations
}

fn model_yaml(yaml: &mut String, root: &Path) -> Result<(), String> {
    // shortcut: nested junction prefixes still need backend traversal; revisit if ComfyUI supports OS448-safe traversal.
    let mut locations = model_locations(root);
    // Tiny decoders may already live under vae; expose them without moving weights or changing scans.
    locations.push(("vae_approx", root.join("vae")));
    for category in MODEL_FOLDERS {
        let paths: Vec<_> = locations
            .iter()
            .filter(|(c, _)| *c == category)
            .map(|(_, path)| {
                let target = match std::fs::symlink_metadata(path) {
                    Ok(_) => crate::downloads::models::backend_directory_target(path)?,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e.to_string()),
                };
                let path = target
                    .as_deref()
                    .unwrap_or_else(|| path.strip_prefix(root).unwrap());
                path.to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Non-UTF-8 model directory".to_owned())
            })
            .collect::<Result<_, String>>()?;
        if paths.iter().any(|path| Path::new(path).is_absolute()) {
            // Escape NEL too: JSON leaves it literal, but YAML normalizes it to a space.
            yaml.push_str(&format!(
                "  {category}: {}\n",
                serde_json::to_string(&paths.join("\n"))
                    .map_err(|e| e.to_string())?
                    .replace('\u{85}', "\\u0085")
            ));
            continue;
        }
        match paths.as_slice() {
            [] => {}
            [path] => yaml.push_str(&format!("  {category}: {path}\n")),
            paths => {
                yaml.push_str(&format!("  {category}: |\n"));
                for path in paths {
                    yaml.push_str(&format!("    {path}\n"));
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Folders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Additional model roots on other drives, registered beside the primary models root in
    /// ComfyUI's extra_model_paths.yaml and scanned by the model library. They are external
    /// folders: the folder mover never copies, deletes or relocates them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_models: Vec<String>,
}

fn existing_dir(value: &Option<String>) -> Result<(), String> {
    let Some(path) = value else { return Ok(()) };
    let path = Path::new(path);
    if !path.is_absolute() || path.to_str().is_none() {
        return Err("Folder must be an absolute path".into());
    }
    if !path.is_dir() {
        return Err(format!("{}: not an existing directory", path.display()));
    }
    Ok(())
}

fn existing_root(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() || path.to_str().is_none() {
        return Err("Folder must be an absolute path".into());
    }
    if !path.is_dir() {
        return Err(format!("{}: not an existing directory", path.display()));
    }
    Ok(path.to_owned())
}

fn root_dir(root: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(root);
    if !root.is_absolute() {
        return Err("Environment root must be absolute".into());
    }
    Ok(root)
}

pub fn read(root: &Path) -> Result<Folders, String> {
    match std::fs::read(root.join(FILE)) {
        Ok(bytes) => serde_json::from_slice(crate::bom_free(&bytes))
            .map_err(|e| format!("Invalid {FILE}: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Folders::default()),
        Err(e) => Err(format!("{FILE}: {e}")),
    }
}

#[path = "settings_move.rs"]
mod relocation;
pub use relocation::{move_folders, validate, MoveReport};

pub fn write(root: &Path, folders: Folders) -> Result<Folders, String> {
    relocation::save(root, folders)
}

/// Extra ComfyUI launch arguments honouring the saved folders; empty when nothing is set.
pub fn backend_args(root: &Path) -> Result<Vec<String>, String> {
    backend_configuration(root).map(|(args, _)| args)
}

pub(crate) fn backend_configuration(root: &Path) -> Result<(Vec<String>, Vec<PathBuf>), String> {
    let folders = read(root)?;
    // ComfyUI keeps its built-in root first, then appends our configured roots.
    let mut model_roots = vec![root.join("ComfyUI/models")];
    model_roots.extend(folders.models.iter().map(PathBuf::from));
    model_roots.extend(folders.extra_models.iter().map(PathBuf::from));
    for value in [&folders.models, &folders.input, &folders.output] {
        existing_dir(value)?; // a folder removed since saving must fail the start, not silently fall back
    }
    for extra in &folders.extra_models {
        existing_root(extra)?;
    }
    let mut args = vec![];
    if let Some(input) = folders.input {
        args.extend(["--input-directory".to_owned(), input]);
    }
    if let Some(output) = folders.output {
        args.extend(["--output-directory".to_owned(), output]);
    }
    // ComfyUI's extra_model_paths format: one named block per root with base_path + folder keys.
    let mut yaml = String::new();
    if let Some(models) = &folders.models {
        yaml.push_str(&format!(
            "deas:\n  base_path: {}\n",
            serde_json::to_string(models).map_err(|e| e.to_string())?
        ));
        model_yaml(&mut yaml, Path::new(models))?;
        // The node pack's Prompt Forge reads `llm`; not in MODEL_FOLDERS so model scans skip LLM weights.
        yaml.push_str("  llm: llm\n");
    }
    for (index, extra) in folders.extra_models.iter().enumerate() {
        yaml.push_str(&format!(
            "deas-extra-{}:\n  base_path: {}\n",
            index + 1,
            serde_json::to_string(extra).map_err(|e| e.to_string())?
        ));
        model_yaml(&mut yaml, Path::new(extra))?;
    }
    if !yaml.is_empty() {
        let file = root.join("deas-models.yaml");
        std::fs::write(&file, yaml).map_err(|e| e.to_string())?;
        args.extend([
            "--extra-model-paths-config".to_owned(),
            file.to_str().ok_or("Non-UTF-8 root")?.to_owned(),
        ]);
    }
    Ok((args, model_roots))
}

#[tauri::command]
pub fn native_settings_read(root: String) -> Result<Folders, String> {
    read(&root_dir(&root)?)
}
#[tauri::command]
pub fn native_settings_write(
    root: String,
    folders: Folders,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<Folders, String> {
    let _guard = operations.enter()?;
    write(&root_dir(&root)?, folders)
}
/// Which of the given absolute file paths already exist; used to mark library items as local.
#[tauri::command]
pub fn native_files_present(paths: Vec<String>) -> Result<Vec<bool>, String> {
    paths
        .iter()
        .take(500)
        .map(|p| {
            let path = Path::new(p);
            if !path.is_absolute() {
                return Err("Path must be absolute".into());
            }
            Ok(path.is_file())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn bom_prefixed_settings_file_still_reads() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("models")).unwrap();
        let folders = Folders {
            models: Some(tmp.path().join("models").to_str().unwrap().into()),
            ..Folders::default()
        };
        write(tmp.path(), folders.clone()).unwrap();
        let file = tmp.path().join(FILE);
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, [b"\xef\xbb\xbf".as_slice(), &bytes].concat()).unwrap();
        assert_eq!(read(tmp.path()).unwrap(), folders);
    }
    #[test]
    fn swarm_backend_yaml_maps_aliases_inside_category_containing_roots() {
        let tmp = TempDir::new().unwrap();
        let models = tmp.path().join("Swarm models");
        let extra_root = tmp.path().join("extra models");
        std::fs::create_dir_all(models.join("Stable-Diffusion")).unwrap();
        std::fs::create_dir_all(extra_root.join("Lora")).unwrap();
        write(
            tmp.path(),
            Folders {
                models: Some(models.to_str().unwrap().into()),
                extra_models: vec![extra_root.to_str().unwrap().into()],
                ..Folders::default()
            },
        )
        .unwrap();
        backend_configuration(tmp.path()).unwrap();
        let yaml = std::fs::read_to_string(tmp.path().join("deas-models.yaml")).unwrap();
        assert!(
            yaml.contains("  checkpoints: |\n    checkpoints\n    Stable-Diffusion\n"),
            "{yaml}"
        );
        let extra = yaml.split("deas-extra-1:").nth(1).unwrap();
        assert!(
            extra.contains("  loras: |\n    loras\n    Lora\n"),
            "{extra}"
        );
        assert!(extra.contains("  checkpoints: checkpoints\n"));
        assert!(yaml.contains("  llm: llm\n"));
    }

    fn junction_configuration_fixture(root: &Path) -> Vec<(&'static str, String)> {
        let primary = root.join("primary");
        let extra = root.join("extra");
        std::fs::create_dir(&primary).unwrap();
        std::fs::create_dir(&extra).unwrap();
        let mut expected = vec![];
        for (index, (models, folder, category)) in [
            (&primary, "checkpoints", "checkpoints"),
            (&primary, "Lora", "loras"),
            (&extra, "Stable-Diffusion", "checkpoints"),
            (&extra, "loras", "loras"),
            (&primary, "vae_approx", "vae_approx"),
            (&extra, "vae", "vae"),
        ]
        .into_iter()
        .enumerate()
        {
            let target = root.join(format!("actual # café \u{85} {index}"));
            std::fs::create_dir(&target).unwrap();
            let name = format!("backend-exact-{index}.safetensors");
            let header = br#"{"__metadata__":{"modelspec.architecture":"stable-diffusion-xl"}}"#;
            let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
            bytes.extend(header);
            std::fs::write(target.join(&name), bytes).unwrap();
            let alias = models.join(folder);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &alias).unwrap();
            #[cfg(windows)]
            {
                let output = std::process::Command::new("cmd.exe")
                    .args(["/D", "/C", "mklink", "/J"])
                    .arg(&alias)
                    .arg(&target)
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{output:?}");
            }
            expected.push((category, name.clone()));
            if category == "vae" {
                expected.push(("vae_approx", name));
            }
        }
        write(
            root,
            Folders {
                models: Some(primary.to_str().unwrap().into()),
                extra_models: vec![extra.to_str().unwrap().into()],
                ..Folders::default()
            },
        )
        .unwrap();
        backend_configuration(root).unwrap();
        expected
    }

    #[test]
    fn backend_yaml_resolves_category_links_and_quotes_multiline_targets() {
        let tmp = TempDir::new().unwrap();
        junction_configuration_fixture(tmp.path());
        let yaml = std::fs::read_to_string(tmp.path().join("deas-models.yaml")).unwrap();
        for index in 0..6 {
            let target =
                dunce::canonicalize(tmp.path().join(format!("actual # café \u{85} {index}")))
                    .unwrap();
            let escaped = serde_json::to_string(target.to_str().unwrap())
                .unwrap()
                .replace('\u{85}', "\\u0085");
            assert!(yaml.contains(&escaped[1..escaped.len() - 1]), "{yaml}");
        }
        assert!(yaml.contains("  loras: \"loras\\n"), "{yaml}");
        assert!(!yaml.contains("  checkpoints: checkpoints\n"), "{yaml}");
        assert!(yaml.contains("  vae: vae\n"));
        assert!(!yaml.contains('\u{85}'));
        assert!(yaml.contains("\\u0085"));
    }

    #[test]
    #[ignore = "Requires actual owned ComfyUI Python and source paths in DEAS_BACKEND_PYTHON / DEAS_COMFYUI"]
    fn live_backend_config_parser_reads_category_junction_models() {
        let tmp = TempDir::new().unwrap();
        let expected = junction_configuration_fixture(tmp.path());
        let script = r#"
import sys,json,os
sys.path.insert(0,sys.argv[1])
import folder_paths
from utils.extra_config import load_extra_path_config
load_extra_path_config(sys.argv[2])
results=[]
for category,name in json.loads(sys.argv[3]):
    names=folder_paths.get_filename_list(category)
    assert name in names,(category,name,names)
    path=folder_paths.get_full_path(category,name)
    assert path,(category,name)
    with open(path,'rb') as model:
        size=int.from_bytes(model.read(8),'little')
        header=json.loads(model.read(size))
    assert header['__metadata__']['modelspec.architecture']=='stable-diffusion-xl'
    results.append(dict(category=category,name=name,path=path,readableHeader=True))
print('ACTUAL_BACKEND '+json.dumps(results,ensure_ascii=False))
if os.name=='nt':
    import subprocess,shutil
    target=os.path.join(os.path.dirname(sys.argv[2]),'nested target')
    os.mkdir(target)
    source=folder_paths.get_full_path('loras','backend-exact-1.safetensors')
    shutil.copyfile(source,os.path.join(target,'nested-backend.safetensors'))
    alias=os.path.join(os.path.dirname(source),'nested')
    subprocess.run(['cmd.exe','/D','/C','mklink','/J',alias,target],check=True,capture_output=True)
    folder_paths.filename_list_cache.clear()
    result={'name':'nested/nested-backend.safetensors'}
    for key,operation in [
        ('listed',lambda:result['name'] in folder_paths.get_filename_list('loras')),
        ('path',lambda:folder_paths.get_full_path('loras',result['name']))
    ]:
        try: result[key]=operation()
        except OSError as error: result[key]={'winerror':error.winerror,'error':str(error)}
    print('NESTED_BACKEND '+json.dumps(result,ensure_ascii=False))
"#;
        let output = std::process::Command::new(std::env::var("DEAS_BACKEND_PYTHON").unwrap())
            .arg("-c")
            .arg(script)
            .arg(std::env::var("DEAS_COMFYUI").unwrap())
            .arg(tmp.path().join("deas-models.yaml"))
            .arg(serde_json::to_string(&expected).unwrap())
            .env("PYTHONIOENCODING", "utf-8")
            .output()
            .unwrap();
        println!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn preview_vae_categories_register_all_roots_without_changing_weights_or_scan_categories() {
        let tmp = TempDir::new().unwrap();
        let primary = tmp.path().join("models");
        let extra = tmp.path().join("extra models");
        for root in [&primary, &extra] {
            for category in ["vae", "vae_approx"] {
                let directory = root.join(category).join("MiniMaxH3");
                std::fs::create_dir_all(&directory).unwrap();
                std::fs::write(directory.join("taeh3.safetensors"), b"original fixture").unwrap();
            }
        }
        write(
            tmp.path(),
            Folders {
                models: Some(primary.to_str().unwrap().into()),
                extra_models: vec![extra.to_str().unwrap().into()],
                ..Folders::default()
            },
        )
        .unwrap();
        backend_args(tmp.path()).unwrap();
        let yaml = std::fs::read_to_string(tmp.path().join("deas-models.yaml")).unwrap();
        for block in ["deas:", "deas-extra-1:"] {
            let block = yaml
                .split(block)
                .nth(1)
                .unwrap()
                .split("\ndeas")
                .next()
                .unwrap();
            assert!(block.contains("  vae: vae\n"), "{block}");
            assert!(
                block.contains("  vae_approx: |\n    vae_approx\n    vae\n"),
                "{block}"
            );
        }
        assert_eq!(model_folder_category("vae_approx"), Some("vae_approx"));
        for root in [&primary, &extra] {
            let models = crate::downloads::models::scan_roots(std::slice::from_ref(root)).unwrap();
            assert_eq!(models.len(), 2);
            for category in ["vae", "vae_approx"] {
                assert_eq!(
                    models
                        .iter()
                        .filter(|model| model.category == category)
                        .count(),
                    1
                );
                let directory = root.join(category).join("MiniMaxH3");
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
                assert_eq!(
                    std::fs::read(directory.join("taeh3.safetensors")).unwrap(),
                    b"original fixture"
                );
            }
        }
    }

    #[test]
    fn latent_upscale_category_is_registered_and_scanned_for_all_model_roots() {
        let tmp = TempDir::new().unwrap();
        let primary = tmp.path().join("models");
        let extra = tmp.path().join("extra models");
        for root in [&primary, &extra] {
            std::fs::create_dir_all(root.join("latent_upscale_models")).unwrap();
            std::fs::write(
                root.join("latent_upscale_models/upscaler.safetensors"),
                b"fixture",
            )
            .unwrap();
        }
        write(
            tmp.path(),
            Folders {
                models: Some(primary.to_str().unwrap().into()),
                extra_models: vec![extra.to_str().unwrap().into()],
                ..Folders::default()
            },
        )
        .unwrap();
        backend_args(tmp.path()).unwrap();
        let yaml = std::fs::read_to_string(tmp.path().join("deas-models.yaml")).unwrap();
        for block in ["deas:", "deas-extra-1:"] {
            let block = yaml
                .split(block)
                .nth(1)
                .unwrap()
                .split("\ndeas")
                .next()
                .unwrap();
            assert!(block.contains("  latent_upscale_models: latent_upscale_models\n"));
            // YuE2 covers load SheetSage2 from the configured models root.
            assert!(block.contains("  audio_encoders: audio_encoders\n"));
        }
        let models = crate::downloads::models::scan_roots(&[primary, extra]).unwrap();
        assert_eq!(models.len(), 2);
        assert!(models
            .iter()
            .all(|model| model.category == "latent_upscale_models"));
    }
    #[test]
    fn extra_model_roots_register_beside_the_primary_root() {
        let tmp = TempDir::new().unwrap();
        let extra = tmp.path().join("extra drive models");
        std::fs::create_dir_all(extra.join("diffusion_models")).unwrap();
        let folders = Folders {
            extra_models: vec![extra.to_str().unwrap().into()],
            ..Folders::default()
        };
        validate(&folders).unwrap();
        write(tmp.path(), folders.clone()).unwrap();
        let (args, roots) = backend_configuration(tmp.path()).unwrap();
        assert_eq!(
            roots,
            vec![
                tmp.path().join("ComfyUI/models"),
                dunce::canonicalize(&extra).unwrap()
            ]
        );
        let yaml = std::fs::read_to_string(&args[1]).unwrap();
        assert!(yaml.contains("deas-extra-1:\n"));
        // Saved folders are canonical (Windows temp may be an 8.3 short path), so compare that form.
        let saved = dunce::canonicalize(&extra).unwrap();
        assert!(yaml.contains(&format!(
            "base_path: {}",
            serde_json::to_string(&saved.to_str().unwrap()).unwrap()
        )));
        assert!(yaml.contains("  diffusion_models: diffusion_models\n"));
        let missing = Folders {
            extra_models: vec![tmp.path().join("gone").to_str().unwrap().into()],
            ..Folders::default()
        };
        assert!(validate(&missing).is_err());
        assert!(write(tmp.path(), missing).is_err());
        std::fs::remove_dir_all(&extra).unwrap();
        assert!(backend_args(tmp.path()).is_err());
    }
    #[test]
    fn settings_roundtrip_validation_and_backend_args() {
        // Saved folders are canonical (Windows: long names, no \\?\ prefix), so compare against that form.
        let (root_dir, models_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let root = dunce::canonicalize(root_dir.path()).unwrap();
        let models = dunce::canonicalize(models_dir.path()).unwrap();
        assert_eq!(read(root.as_path()).unwrap(), Folders::default());
        assert!(backend_args(root.as_path()).unwrap().is_empty());
        assert!(write(
            root.as_path(),
            Folders {
                models: Some("relative".into()),
                ..Default::default()
            }
        )
        .is_err());
        assert!(write(
            root.as_path(),
            Folders {
                input: Some(root.as_path().join("missing").to_str().unwrap().into()),
                ..Default::default()
            }
        )
        .is_err());
        let saved = write(
            root.as_path(),
            Folders {
                models: Some(models.as_path().to_str().unwrap().into()),
                output: Some(root.as_path().to_str().unwrap().into()),
                input: None,
                extra_models: vec![],
            },
        )
        .unwrap();
        assert_eq!(read(root.as_path()).unwrap(), saved);
        let args = backend_args(root.as_path()).unwrap();
        assert_eq!(
            args[0..2],
            [
                "--output-directory".to_owned(),
                root.as_path().to_str().unwrap().to_owned()
            ]
        );
        assert_eq!(args[2], "--extra-model-paths-config");
        let yaml = std::fs::read_to_string(&args[3]).unwrap();
        assert!(yaml.starts_with(&format!(
            "deas:\n  base_path: {}\n  checkpoints: checkpoints\n",
            serde_json::to_string(models.as_path().to_str().unwrap()).unwrap()
        )));
        assert!(yaml.contains("  loras: loras\n"));
        assert!(yaml.contains("  llm: llm\n"));
        let present = native_files_present(vec![
            args[3].clone(),
            root.as_path().join("nope").to_str().unwrap().into(),
        ])
        .unwrap();
        assert_eq!(present, vec![true, false]);
        assert!(native_files_present(vec!["relative".into()]).is_err());
        assert!(native_settings_read("relative".into()).is_err());
        drop(models_dir);
        assert!(backend_args(root.as_path()).is_err());
    }
}
