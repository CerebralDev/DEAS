//! Model-family detection driven by the data-only family plugin `src/families/builtin.json`
//! (shared with the frontend, which reads loaders, pipelines and defaults from it). Like SwarmUI's
//! class sorter it reads only the safetensors header: `modelspec.architecture` first, then key
//! signatures; the cached CivitAI base model is the fallback for other formats.
use serde::Deserialize;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;

/// Headers of large quantized checkpoints stay well below this; anything bigger is not trusted.
const MAX_HEADER: u64 = 64 << 20;
const MAX_STRING: u64 = 1 << 24;
const MAX_VALUES: u64 = 1 << 20;

#[derive(Deserialize)]
struct Family {
    id: String,
    loader: String,
    #[serde(default)]
    forge: Option<String>,
    #[serde(default, rename = "clipType")]
    clip_type: Option<String>,
    #[serde(default)]
    multiple: Option<u64>,
    #[serde(default, rename = "temporalLatent")]
    temporal_latent: bool,
    architectures: Vec<String>,
    bases: Vec<String>,
    model: Vec<Vec<String>>,
    lora: Vec<Vec<String>>,
    /// Lowercase file-name substrings for variants that share a header with their base
    /// (Qwen Image 2.1 Turbo declares the same architecture as the base model).
    #[serde(default, rename = "fileMatch")]
    file_match: Vec<String>,
    /// Latent node the family's image recipe creates; `EmptyLatentImage` when absent.
    #[serde(default)]
    latent: Option<String>,
    /// `"zero"` replaces the encoded negative with `ConditioningZeroOut` (cfg-1 distilled models).
    #[serde(default)]
    negative: Option<String>,
    /// Model-sampling patch the family's recipe chains after the model (e.g. `ModelSamplingAuraFlow`).
    #[serde(default)]
    patch: Option<Patch>,
    /// Header tensor shapes that must match, because architectures share a key set (Z-Image 3840 vs Lumina2 2304).
    #[serde(default, rename = "modelShapes")]
    model_shapes: Vec<ShapeRule>,
    /// Scheduler node that derives the sigmas from steps/canvas (Flux2Scheduler), replacing KSampler.
    #[serde(default, rename = "sigmaNode")]
    sigma_node: Option<String>,
    /// Conditioning node that also carries the latent (Mage-Flow's TextEncodeMageFlowEdit: outputs positive/negative/latent).
    #[serde(default, rename = "encodeNode")]
    encode_node: Option<String>,
}

#[derive(Deserialize, Default)]
struct ShapeRule {
    key: String,
    /// Tensor shape prefix; an empty list matches any shape.
    #[serde(default)]
    shape: Vec<u64>,
}

#[derive(Deserialize, Default)]
struct Patch {
    node: String,
    #[serde(default)]
    shift: Option<f64>,
}

/// Image-recipe points a family declares instead of a compiler branch; the defaults are the
/// plain checkpoint graph every unknown architecture keeps.
#[derive(Default)]
pub(crate) struct ImageRecipe {
    pub(crate) latent: Option<&'static str>,
    pub(crate) temporal_latent: bool,
    pub(crate) zero_negative: bool,
    pub(crate) patch: Option<(&'static str, f64)>,
    pub(crate) sigma_node: Option<&'static str>,
    pub(crate) encode_node: Option<&'static str>,
}

pub(crate) fn image_recipe(clip_type: &str) -> ImageRecipe {
    families()
        .iter()
        .find(|f| f.clip_type.as_deref() == Some(clip_type))
        .map(recipe_of)
        .unwrap_or_default()
}

/// Recipe by explicit family id: families that share a CLIP type (Chroma / Chroma Radiance) differ in latent.
pub(crate) fn image_recipe_by_id(id: &str) -> Option<ImageRecipe> {
    families().iter().find(|f| f.id == id).map(recipe_of)
}

fn recipe_of(f: &'static Family) -> ImageRecipe {
    ImageRecipe {
        latent: f.latent.as_deref(),
        temporal_latent: f.temporal_latent,
        zero_negative: f.negative.as_deref() == Some("zero"),
        patch: f
            .patch
            .as_ref()
            .map(|p| (p.node.as_str(), p.shift.unwrap_or(1.0))),
        sigma_node: f.sigma_node.as_deref(),
        encode_node: f.encode_node.as_deref(),
    }
}

fn families() -> &'static [Family] {
    static FAMILIES: OnceLock<Vec<Family>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        serde_json::from_str(include_str!("../../src/families/builtin.json"))
            .expect("bundled family plugin is valid")
    })
}

/// Declarations select existing recipe families, never companion architectures or new loaders.
pub(crate) fn validate_override(category: &str, id: &str) -> Result<(), String> {
    let family = families()
        .iter()
        .find(|f| f.id == id)
        .ok_or("Unknown model family")?;
    if !["checkpoint", "diffusion"].contains(&family.loader.as_str())
        || match category {
            "loras" => family.lora.is_empty(),
            "checkpoints" | "diffusion_models" | "unet" => false,
            _ => true,
        }
    {
        return Err("Model category does not support this family declaration".into());
    }
    Ok(())
}

pub(crate) fn prompt_forge_target(name: &str) -> Option<&'static str> {
    let name = name.trim();
    families()
        .iter()
        .find(|f| {
            f.id.eq_ignore_ascii_case(name)
                || f.forge
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(name))
                || f.bases.iter().any(|v| v.eq_ignore_ascii_case(name))
        })?
        .forge
        .as_deref()
}

pub(crate) fn image_multiple(clip_type: &str) -> Option<u64> {
    families()
        .iter()
        .find(|f| f.clip_type.as_deref() == Some(clip_type))?
        .multiple
}

fn read_header(path: &Path) -> Option<(serde_json::Map<String, Value>, u64)> {
    let mut file = crate::downloads::models::open_model_file(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() {
        return None;
    }
    let mut len = [0u8; 8];
    file.read_exact(&mut len).ok()?;
    let len = u64::from_le_bytes(len);
    if len > MAX_HEADER || len > meta.len().checked_sub(8)? {
        return None;
    }
    let mut buf = vec![0; len as usize];
    file.read_exact(&mut buf).ok()?;
    Some((serde_json::from_slice(&buf).ok()?, meta.len() - 8 - len))
}

/// Folded tensor name → shape, read once per file.
type TensorShapes = Vec<(String, Vec<u64>)>;

/// Header keys joined with `_` folded to `.`, preserving the existing main-family policy; tensor shapes are folded the same way.
fn header(path: &Path) -> Option<(String, String, TensorShapes)> {
    let (head, _) = read_header(path)?;
    let arch = head
        .get("__metadata__")
        .and_then(|m| m.get("modelspec.architecture"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let shapes = head
        .iter()
        .filter(|(k, _)| k.as_str() != "__metadata__")
        .map(|(k, tensor)| {
            (
                k.replace('_', "."),
                tensor
                    .get("shape")
                    .and_then(Value::as_array)
                    .map(|s| s.iter().filter_map(Value::as_u64).collect::<Vec<u64>>())
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<(String, Vec<u64>)>>();
    Some((
        head.keys()
            .map(|k| k.replace('_', "."))
            .collect::<Vec<_>>()
            .join("\n"),
        arch,
        shapes,
    ))
}

/// `modelShapes` rules narrow a key-set match: every rule's tensor must exist with the declared shape prefix.
fn shapes_match(f: &Family, shapes: &TensorShapes) -> bool {
    f.model_shapes.iter().all(|rule| {
        let key = rule.key.replace('_', ".");
        shapes.iter().any(|(name, shape)| {
            name.ends_with(&key)
                && shape.len() >= rule.shape.len()
                && shape[..rule.shape.len()] == rule.shape[..]
        })
    })
}

/// Validate tensor descriptors and payload bounds without reading any weight bytes.
fn valid_tensors(head: &serde_json::Map<String, Value>, payload: u64) -> Option<()> {
    let mut ranges = Vec::new();
    for (key, tensor) in head {
        if key == "__metadata__" {
            if !tensor.as_object()?.values().all(Value::is_string) {
                return None;
            }
            continue;
        }
        let width = match tensor.get("dtype")?.as_str()? {
            "BOOL" | "U8" | "I8" | "F8_E4M3" | "F8_E5M2" | "F8_E4M3FN" => 1,
            "BF16" | "F16" | "I16" | "U16" => 2,
            "F32" | "I32" | "U32" => 4,
            "F64" | "I64" | "U64" => 8,
            _ => return None,
        };
        let shape = tensor.get("shape")?.as_array()?;
        if shape.len() > 8 {
            return None;
        }
        let bytes = shape
            .iter()
            .try_fold(width, |n: u64, d| n.checked_mul(d.as_u64()?))?;
        let offsets = tensor.get("data_offsets")?.as_array()?;
        if offsets.len() != 2 {
            return None;
        }
        let (start, end) = (offsets[0].as_u64()?, offsets[1].as_u64()?);
        if end.checked_sub(start)? != bytes || end > payload {
            return None;
        }
        ranges.push((start, end));
    }
    ranges.sort_unstable();
    let mut end = 0;
    for (start, next) in ranges {
        if start != end {
            return None;
        }
        end = next;
    }
    (end == payload).then_some(())
}

/// safetensors payload completeness: header parses and every tensor range lies inside the file.
/// `None` for other formats; `Some(false)` for a truncated/partial safetensors.
pub fn intact(path: &Path) -> Option<bool> {
    if !safetensors(path) {
        return None;
    }
    Some(
        read_header(path)
            .and_then(|(head, payload)| valid_tensors(&head, payload))
            .is_some(),
    )
}

fn safetensors(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["safetensors", "sft"].contains(&e.to_ascii_lowercase().as_str()))
}

fn gguf_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
}

struct GgufReader<R> {
    inner: R,
    position: u64,
    end: u64,
}

impl<R: Read + Seek> GgufReader<R> {
    fn checked_end(&self, bytes: u64) -> Option<u64> {
        let end = self.position.checked_add(bytes)?;
        (end <= self.end).then_some(end)
    }

    fn read<const N: usize>(&mut self) -> Option<[u8; N]> {
        let end = self.checked_end(N as u64)?;
        let mut bytes = [0; N];
        self.inner.read_exact(&mut bytes).ok()?;
        self.position = end;
        Some(bytes)
    }

    fn u32v(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.read()?))
    }

    fn u64v(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.read()?))
    }

    fn skip(&mut self, bytes: u64) -> Option<()> {
        let end = self.checked_end(bytes)?;
        self.inner.seek(SeekFrom::Start(end)).ok()?;
        self.position = end;
        Some(())
    }

    fn string(&mut self) -> Option<String> {
        let len = self.u64v()?;
        if len > MAX_STRING {
            return None;
        }
        let end = self.checked_end(len)?;
        let mut bytes = vec![0; usize::try_from(len).ok()?];
        self.inner.read_exact(&mut bytes).ok()?;
        self.position = end;
        String::from_utf8(bytes).ok()
    }
}

fn gguf_scalar_width(vtype: u32) -> Option<u64> {
    match vtype {
        0 | 1 | 7 => Some(1),
        2 | 3 => Some(2),
        4..=6 => Some(4),
        10..=12 => Some(8),
        _ => None,
    }
}

fn skip_gguf_value<R: Read + Seek>(r: &mut GgufReader<R>, vtype: u32) -> Option<()> {
    if let Some(width) = gguf_scalar_width(vtype) {
        r.skip(width)
    } else if vtype == 8 {
        r.string().map(|_| ())
    } else if vtype == 9 {
        let elem = r.u32v()?;
        let count = r.u64v()?;
        if count > MAX_VALUES {
            return None;
        }
        if let Some(width) = gguf_scalar_width(elem) {
            r.skip(count.checked_mul(width)?)
        } else if elem == 8 {
            r.checked_end(count.checked_mul(8)?)?;
            for _ in 0..count {
                r.string()?;
            }
            Some(())
        } else {
            // GGUF arrays cannot contain arrays, even when empty.
            None
        }
    } else {
        None
    }
}

/// KV metadata and tensor names of a GGUF, weight bytes never read. Quantizers preserve the
/// source tensor names, so detection reuses the safetensors key policy; `general.architecture`
/// is the quantizer's free-form label. Unreadable metadata stays unknown, never a filename guess.
fn gguf_metadata(path: &Path) -> Option<(String, String)> {
    let file = crate::downloads::models::open_model_file(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() {
        return None;
    }
    // No buffered read-ahead: the last descriptor must not pull in tensor payload bytes.
    let mut r = GgufReader {
        inner: file,
        position: 0,
        end: meta.len().min(MAX_HEADER),
    };
    if &r.read::<4>()? != b"GGUF" || !matches!(r.u32v()?, 2 | 3) {
        return None;
    }
    let tensors = r.u64v()?;
    let kv = r.u64v()?;
    if tensors > MAX_VALUES || kv > MAX_VALUES {
        return None;
    }
    let mut arch = String::new();
    for _ in 0..kv {
        let key = r.string()?;
        let vtype = r.u32v()?;
        if key == "general.architecture" && vtype == 8 {
            arch = r.string()?.to_ascii_lowercase();
        } else {
            skip_gguf_value(&mut r, vtype)?;
        }
    }
    let mut keys = String::new();
    for index in 0..tensors {
        let key = r.string()?;
        let separator = usize::from(index != 0);
        if keys.len().checked_add(separator)?.checked_add(key.len())? > MAX_HEADER as usize {
            return None;
        }
        if separator != 0 {
            keys.push('\n');
        }
        keys.push_str(&key.replace('_', "."));
        let dims = r.u32v()?;
        if !(1..=4).contains(&dims) {
            return None;
        }
        // shortcut: descriptor bytes only; validate types/offsets/payload before claiming load readiness.
        r.skip(u64::from(dims) * 8 + 12)?;
    }
    Some((keys, arch))
}

#[cfg(test)]
mod gguf_parser_tests {
    use super::*;

    // Independent spec-v3.gguf from the GGUF audit, not write_gguf's encoding.
    const SPEC: &[u8] = &[
        71, 71, 85, 70, 3, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0,
        0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 97, 114, 99, 104, 105, 116, 101, 99, 116,
        117, 114, 101, 8, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 119, 97, 110, 28, 0, 0, 0, 0, 0, 0, 0,
        98, 108, 111, 99, 107, 115, 46, 48, 46, 99, 114, 111, 115, 115, 95, 97, 116, 116, 110, 46,
        107, 46, 119, 101, 105, 103, 104, 116, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 63,
    ];

    fn parse(bytes: &[u8]) -> Option<(String, String)> {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::var_os("TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = dir.join(format!(
            "deas-gguf-{}-{}.gguf",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        let result = gguf_metadata(&path);
        std::fs::remove_file(path).unwrap();
        result
    }

    fn string(bytes: &mut Vec<u8>, value: &str) {
        bytes.extend((value.len() as u64).to_le_bytes());
        bytes.extend(value.as_bytes());
    }

    fn metadata(value_type: u32, value: &[u8]) -> Vec<u8> {
        let mut bytes = b"GGUF".to_vec();
        bytes.extend(3u32.to_le_bytes());
        bytes.extend(0u64.to_le_bytes());
        bytes.extend(2u64.to_le_bytes());
        string(&mut bytes, "ignored");
        bytes.extend(value_type.to_le_bytes());
        bytes.extend(value);
        string(&mut bytes, "general.architecture");
        bytes.extend(8u32.to_le_bytes());
        string(&mut bytes, "WAN");
        bytes
    }

    #[test]
    fn gguf_scalar_widths_and_flat_arrays() {
        for (tag, width) in [
            (0u32, 1),
            (1, 1),
            (2, 2),
            (3, 2),
            (4, 4),
            (5, 4),
            (6, 4),
            (7, 1),
            (10, 8),
            (11, 8),
            (12, 8),
        ] {
            assert_eq!(
                parse(&metadata(tag, &vec![0; width])),
                Some((String::new(), "wan".into())),
                "scalar {tag}"
            );
            let mut array = tag.to_le_bytes().to_vec();
            array.extend(3u64.to_le_bytes());
            array.extend(vec![0; width * 3]);
            assert_eq!(
                parse(&metadata(9, &array)),
                Some((String::new(), "wan".into())),
                "array {tag}"
            );
        }
        let mut value = Vec::new();
        string(&mut value, "abc");
        assert!(parse(&metadata(8, &value)).is_some());
        let mut array = 8u32.to_le_bytes().to_vec();
        array.extend(2u64.to_le_bytes());
        string(&mut array, "one");
        string(&mut array, "two");
        assert!(parse(&metadata(9, &array)).is_some());
    }

    #[test]
    fn gguf_rejects_every_truncated_header_prefix() {
        // This fixture has one alignment byte plus four tensor data bytes after its descriptors.
        let header_end = SPEC.len() - 5;
        for end in 0..header_end {
            assert_eq!(parse(&SPEC[..end]), None, "prefix {end}");
        }
        assert!(parse(&SPEC[..header_end]).is_some());
    }

    #[test]
    fn gguf_rejects_unsupported_versions_counts_and_value_types() {
        for version in [0u32, 1, 4, u32::MAX] {
            let mut bytes = b"GGUF".to_vec();
            bytes.extend(version.to_le_bytes());
            bytes.extend([0u8; 16]);
            assert_eq!(parse(&bytes), None, "version {version}");
        }
        for dims in [0u32, 5, u32::MAX] {
            let mut bytes = SPEC.to_vec();
            bytes[103..107].copy_from_slice(&dims.to_le_bytes());
            assert_eq!(parse(&bytes), None, "dims {dims}");
        }
        for offset in [8, 16] {
            for count in [2u64, MAX_VALUES + 1, u64::MAX] {
                let mut bytes = SPEC.to_vec();
                bytes[offset..offset + 8].copy_from_slice(&count.to_le_bytes());
                assert_eq!(parse(&bytes), None, "count {offset}/{count}");
            }
        }
        for tag in [13u32, u32::MAX] {
            assert_eq!(parse(&metadata(tag, &[])), None);
        }
        for elem in [9u32, 13, u32::MAX] {
            let mut array = elem.to_le_bytes().to_vec();
            array.extend(0u64.to_le_bytes());
            assert_eq!(parse(&metadata(9, &array)), None);
        }
        for elem in [0u32, 8, 12] {
            for count in [MAX_VALUES + 1, u64::MAX] {
                let mut array = elem.to_le_bytes().to_vec();
                array.extend(count.to_le_bytes());
                assert_eq!(parse(&metadata(9, &array)), None);
            }
        }
        for length in [MAX_STRING + 1, u64::MAX] {
            assert_eq!(parse(&metadata(8, &length.to_le_bytes())), None);
        }
        let mut invalid_utf8 = 1u64.to_le_bytes().to_vec();
        invalid_utf8.push(0xff);
        assert_eq!(parse(&metadata(8, &invalid_utf8)), None);
    }

    #[test]
    fn gguf_rejects_cumulative_header_budget() {
        let mut bytes = b"GGUF".to_vec();
        bytes.extend(3u32.to_le_bytes());
        bytes.extend(0u64.to_le_bytes());
        bytes.extend(4u64.to_le_bytes());
        for key in ["a", "b", "c", "general.architecture"] {
            string(&mut bytes, key);
            bytes.extend(8u32.to_le_bytes());
            bytes.extend(MAX_STRING.to_le_bytes());
            bytes.resize(bytes.len() + MAX_STRING as usize, b'x');
        }
        assert!(bytes.len() as u64 > MAX_HEADER);
        assert!(parse(&bytes).is_none());
    }

    #[test]
    fn gguf_reader_checks_bounds_before_io() {
        let mut r = GgufReader {
            inner: std::io::Cursor::new(vec![0u8; 16]),
            position: 0,
            end: 3,
        };
        assert_eq!(r.u32v(), None);
        assert_eq!(r.inner.position(), 0);
        assert_eq!(r.skip(4), None);
        assert_eq!(r.inner.position(), 0);
        assert_eq!(r.skip(3), Some(()));
        assert_eq!(r.read::<1>(), None);
        assert_eq!(r.inner.position(), 3);
        r.position = u64::MAX;
        r.end = u64::MAX;
        assert_eq!(r.checked_end(1), None);

        let mut bytes = 8u64.to_le_bytes().to_vec();
        bytes.extend([0u8; 8]);
        let mut r = GgufReader {
            inner: std::io::Cursor::new(bytes),
            position: 0,
            end: 15,
        };
        assert_eq!(r.string(), None);
        assert_eq!(r.inner.position(), 8);
    }

    #[test]
    fn gguf_rejects_nonregular_files() {
        let dir = std::env::var_os("TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        assert_eq!(gguf_metadata(&dir), None);
        #[cfg(target_os = "linux")]
        {
            let fifo = dir.join(format!("deas-gguf-fifo-{}", std::process::id()));
            assert!(std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success());
            assert_eq!(gguf_metadata(&fifo), None);
            std::fs::remove_file(fifo).unwrap();
            assert_eq!(gguf_metadata(Path::new("/dev/null")), None);
        }
    }

    #[test]
    fn gguf_independent_v2_v3_fixture() {
        for version in [2u32, 3] {
            let mut bytes = SPEC.to_vec();
            bytes[4..8].copy_from_slice(&version.to_le_bytes());
            assert_eq!(
                parse(&bytes),
                Some(("blocks.0.cross.attn.k.weight".into(), "wan".into()))
            );
        }
    }
}

/// Stable companion architecture ID, not a model-family or inference-readiness claim.
/// Evidence: ComfyUI b1a247c9 comfy/sd.py, text_encoders/{llama,qwen3vl,lt}.py,
/// umt5_config_xxl.json, ldm/wan/{vae,vae2_2}.py and lightricks/vae/audio_vae.py;
/// local headers corroborate shapes.
/// shortcut: packed weights/other architectures stay unknown; add signatures only with evidence.
pub fn detect_component(category: &str, path: &Path) -> Option<&'static str> {
    if !["text_encoders", "clip", "vae"].contains(&category) || !safetensors(path) {
        return None;
    }
    let (head, payload) = read_header(path)?;
    valid_tensors(&head, payload)?;
    let shape = |key: &str, expected: &[u64]| {
        head.get(key)
            .and_then(|v| v.get("shape"))
            .and_then(Value::as_array)
            .is_some_and(|s| {
                s.len() == expected.len()
                    && s.iter().zip(expected).all(|(v, d)| v.as_u64() == Some(*d))
            })
    };
    if category == "vae" {
        // Flux2 packs a 32-channel spatial VAE into 128-channel /16 latents (comfy/sd.py).
        if shape("encoder.conv_in.weight", &[128, 3, 3, 3])
            && shape("decoder.conv_in.weight", &[512, 32, 3, 3])
            && shape("decoder.conv_out.weight", &[3, 128, 3, 3])
            && shape("encoder.quant_conv.weight", &[64, 64, 1, 1])
            && shape("bn.running_mean", &[128])
            && shape("bn.running_var", &[128])
        {
            return Some("flux2-vae");
        }
        // Wan 2.1 and original Qwen Image share this architecture; do not invent separate IDs.
        if shape("conv2.weight", &[16, 16, 1, 1, 1])
            && shape("encoder.conv1.weight", &[96, 3, 3, 3, 3])
            && shape("decoder.conv1.weight", &[384, 16, 3, 3, 3])
            && shape("decoder.middle.0.residual.0.gamma", &[384, 1, 1, 1])
            && shape("decoder.head.2.weight", &[3, 96, 3, 3, 3])
            && !head.contains_key("decoder.upsamples.0.upsamples.0.residual.2.weight")
        {
            return Some("wan-2.1-vae");
        }
        if shape("conv2.weight", &[64, 64, 1, 1, 1])
            && shape("encoder.conv1.weight", &[96, 4, 1, 3, 3])
            && shape("decoder.conv1.weight", &[1152, 64, 1, 3, 3])
            && shape("decoder.middle.0.residual.0.gamma", &[1152, 1, 1, 1])
            && shape("decoder.head.2.weight", &[4, 144, 1, 3, 3])
            && head.contains_key("decoder.upsamples.0.upsamples.0.residual.2.weight")
        {
            return Some("qwen-image-2.1-vae");
        }
        for prefix in ["audio_vae.", "autoencoder."] {
            if shape(
                &format!("{prefix}encoder.conv_in.conv.weight"),
                &[128, 2, 3, 3],
            ) && shape(
                &format!("{prefix}decoder.conv_in.conv.weight"),
                &[512, 8, 3, 3],
            ) && shape(
                &format!("{prefix}decoder.conv_out.conv.weight"),
                &[2, 128, 3, 3],
            ) && shape(
                &format!("{prefix}per_channel_statistics.mean-of-means"),
                &[128],
            ) && (shape(
                "vocoder.vocoder.resblocks.0.convs1.0.weight",
                &[768, 768, 3],
            ) || shape("vocoder.resblocks.0.convs1.0.weight", &[768, 768, 3]))
            {
                return Some("ltx-2.3-audio-vae");
            }
        }
        return None;
    }
    // Qwen3.5-9B hybrid linear/full attention, as selected by MOSS_IMAGE in comfy/sd.py.
    if shape("model.language_model.embed_tokens.weight", &[248320, 4096])
        && shape(
            "model.language_model.layers.0.input_layernorm.weight",
            &[4096],
        )
        && shape("model.language_model.layers.0.linear_attn.A_log", &[32])
        && shape(
            "model.language_model.layers.0.linear_attn.in_proj_qkv.weight",
            &[8192, 4096],
        )
        && shape(
            "model.language_model.layers.31.self_attn.q_proj.weight",
            &[8192, 4096],
        )
        && shape(
            "model.language_model.layers.31.mlp.up_proj.weight",
            &[12288, 4096],
        )
    {
        return Some("qwen3.5-9b");
    }
    // Both raw HF and ComfyUI's flattened Qwen3-VL text prefixes occur in local exports.
    for prefix in ["model.", "model.language_model."] {
        let at = |key: &str, dims: &[u64]| shape(&format!("{prefix}{key}"), dims);
        for (id, hidden, q, intermediate, visual) in [
            ("qwen3-0.6b", 1024, 2048, 3072, None),
            ("qwen3-vl-4b", 2560, 4096, 9728, Some(4096)),
            ("qwen3-vl-8b", 4096, 4096, 12288, Some(4608)),
        ] {
            let last = if visual.is_some() { 35 } else { 27 };
            if !at("embed_tokens.weight", &[151936, hidden])
                || !at("layers.0.post_attention_layernorm.weight", &[hidden])
                || !at("layers.0.self_attn.q_norm.weight", &[128])
                || !at("layers.0.self_attn.q_proj.weight", &[q, hidden])
                || !at(
                    &format!("layers.{last}.mlp.up_proj.weight"),
                    &[intermediate, hidden],
                )
            {
                continue;
            }
            if let Some(visual) = visual {
                if !shape(
                    "model.visual.deepstack_merger_list.0.norm.weight",
                    &[visual],
                ) || !shape("model.visual.merger.linear_fc2.weight", &[hidden, visual])
                {
                    continue;
                }
            } else if head
                .keys()
                .any(|k| k.contains("visual.") || k.starts_with("model.layers.28."))
            {
                continue;
            }
            return Some(id);
        }
    }
    if shape("shared.weight", &[256384, 4096])
        && shape(
            "encoder.block.0.layer.0.SelfAttention.k.weight",
            &[4096, 4096],
        )
        && shape(
            "encoder.block.23.layer.1.DenseReluDense.wi_1.weight",
            &[10240, 4096],
        )
    {
        return Some("umt5-xxl");
    }
    if shape(
        "text_embedding_projection.audio_aggregate_embed.weight",
        &[2048, 188160],
    ) && shape(
        "text_embedding_projection.audio_aggregate_embed.bias",
        &[2048],
    ) && shape(
        "text_embedding_projection.video_aggregate_embed.weight",
        &[4096, 188160],
    ) && shape(
        "text_embedding_projection.video_aggregate_embed.bias",
        &[4096],
    ) {
        return Some("ltx-2.3-text-projection");
    }
    None
}

/// Family ID of a model (`checkpoints`, `diffusion_models`, `unet`) or LoRA file; `None` = unknown.
pub fn detect(category: &str, path: &Path, base: Option<&str>) -> Option<String> {
    let lora = category == "loras";
    if !lora && !["checkpoints", "diffusion_models", "unet"].contains(&category) {
        return None;
    }
    let file = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // File-name variants never claim LoRAs: those stay with the base family they were trained on.
    let all: Vec<&Family> = families()
        .iter()
        .filter(|f| {
            f.file_match.is_empty() || (!lora && f.file_match.iter().any(|s| file.contains(s)))
        })
        .collect();
    let by_header = (if safetensors(path) {
        header(path)
    } else if gguf_file(path) {
        gguf_metadata(path).map(|(keys, arch)| (keys, arch, Vec::new()))
    } else {
        None
    })
    .and_then(|(keys, arch, shapes)| {
        let declared = !arch.is_empty();
        all.iter()
            .find(|f| declared && f.architectures.iter().any(|a| arch.starts_with(a.as_str())))
            .or_else(|| {
                all.iter().find(|f| {
                    shapes_match(f, &shapes)
                        && (if lora { &f.lora } else { &f.model })
                            .iter()
                            .any(|signature| {
                                !signature.is_empty()
                                    && signature
                                        .iter()
                                        .all(|s| keys.contains(&s.replace('_', ".")))
                            })
                })
            })
    });
    by_header
        .or_else(|| {
            base.and_then(|b| {
                all.iter()
                    .find(|f| f.bases.iter().any(|x| x.eq_ignore_ascii_case(b)))
            })
        })
        .map(|f| f.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, header: Value) -> std::path::PathBuf {
        let text = header.to_string();
        let mut bytes = (text.len() as u64).to_le_bytes().to_vec();
        bytes.extend(text.as_bytes());
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn intact_flags_a_truncated_payload_and_accepts_complete_files() {
        let dir = tempfile::tempdir().unwrap();
        let header = serde_json::json!({"w":{"dtype":"F32","shape":[2],"data_offsets":[0,8]}});
        let text = header.to_string();
        let mut bytes = (text.len() as u64).to_le_bytes().to_vec();
        bytes.extend(text.as_bytes());
        let truncated = dir.path().join("t.safetensors");
        std::fs::write(&truncated, &bytes).unwrap();
        assert_eq!(intact(&truncated), Some(false));
        let mut full = bytes.clone();
        full.extend([0u8; 8]);
        let complete = dir.path().join("c.safetensors");
        std::fs::write(&complete, &full).unwrap();
        assert_eq!(intact(&complete), Some(true));
        assert_eq!(intact(&dir.path().join("x.pt")), None);
    }

    #[test]
    fn pixeldit_signature_uses_the_pixel_latent() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let path = write(
            dir.path(),
            "pixeldit_t2i.safetensors",
            serde_json::json!({
                "model.diffusion_model.core.pixel_embedder.proj.weight": t,
            }),
        );
        assert_eq!(
            detect("diffusion_models", &path, None).as_deref(),
            Some("pixeldit")
        );
        assert_eq!(
            image_recipe("pixeldit").latent,
            Some("EmptyChromaRadianceLatentImage")
        );
    }

    #[test]
    fn flux2_and_flux3_signatures_use_the_flux2_scheduler() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let flux2 = write(
            dir.path(),
            "flux-2-klein-base-4b-fp8.safetensors",
            serde_json::json!({
                "model.diffusion_model.double_stream_modulation_img.lin.weight": t.clone(),
                "model.diffusion_model.img_in.weight": t.clone(),
            }),
        );
        assert_eq!(
            detect("diffusion_models", &flux2, None).as_deref(),
            Some("flux2")
        );
        let flux3 = write(
            dir.path(),
            "flux3.safetensors",
            serde_json::json!({
                "model.diffusion_model.content_mode_blocks.image.0.attn_out.weight": t.clone(),
                "model.diffusion_model.single_stream_modulations.txt.lin.weight": t,
            }),
        );
        assert_eq!(
            detect("diffusion_models", &flux3, None).as_deref(),
            Some("flux3")
        );
        for id in ["flux2", "flux3"] {
            let recipe = image_recipe_by_id(id).unwrap();
            assert_eq!(recipe.latent, Some("EmptyFlux2LatentImage"));
            assert_eq!(recipe.sigma_node, Some("Flux2Scheduler"));
        }
    }

    #[test]
    fn fibo2_and_ernie_signatures_use_the_flux2_latent() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let fibo = write(
            dir.path(),
            "fibo2.safetensors",
            serde_json::json!({
                "model.diffusion_model.perceiver.latent_queries": t.clone(),
                "model.diffusion_model.layers.0.attention.qkv.weight": t.clone(),
            }),
        );
        assert_eq!(
            detect("diffusion_models", &fibo, None).as_deref(),
            Some("fibo2")
        );
        let ernie = write(
            dir.path(),
            "ernie_image.safetensors",
            serde_json::json!({
                "model.diffusion_model.layers.0.mlp.linear_fc2.weight": t,
            }),
        );
        assert_eq!(
            detect("diffusion_models", &ernie, None).as_deref(),
            Some("ernie")
        );
        for id in ["fibo2", "ernie"] {
            assert_eq!(
                image_recipe_by_id(id).unwrap().latent,
                Some("EmptyFlux2LatentImage")
            );
        }
    }

    #[test]
    fn omnigen2_signature_is_kept_apart_from_boogu() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let path = write(
            dir.path(),
            "omnigen2.safetensors",
            serde_json::json!({
                "model.diffusion_model.time_caption_embed.timestep_embedder.linear_1.bias": t,
            }),
        );
        assert_eq!(
            detect("diffusion_models", &path, None).as_deref(),
            Some("omnigen2")
        );
        assert_eq!(
            image_recipe_by_id("omnigen2").unwrap().latent,
            Some("EmptySD3LatentImage")
        );
    }

    #[test]
    fn ideogram4_and_boogu_signatures_keep_the_plain_recipe() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let ideogram = write(
            dir.path(),
            "ideogram4.safetensors",
            serde_json::json!({
                "model.diffusion_model.embed_image_indicator.weight": t.clone(),
                "model.diffusion_model.input_proj.weight": t.clone(),
            }),
        );
        assert_eq!(
            detect("diffusion_models", &ideogram, None).as_deref(),
            Some("ideogram4")
        );
        let boogu = write(
            dir.path(),
            "boogu_image.safetensors",
            serde_json::json!({
                "model.diffusion_model.double_stream_layers.0.img_instruct_attn.processor.img_to_q.weight": t.clone(),
                "model.diffusion_model.x_embedder.weight": t,
            }),
        );
        assert_eq!(
            detect("diffusion_models", &boogu, None).as_deref(),
            Some("boogu")
        );
        assert_eq!(
            image_recipe_by_id("ideogram4").unwrap().latent,
            Some("EmptyFlux2LatentImage")
        );
        assert_eq!(
            image_recipe_by_id("boogu").unwrap().latent,
            Some("EmptySD3LatentImage")
        );
    }

    #[test]
    fn kandinsky5_image_signature_keeps_the_plain_recipe() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let path = write(
            dir.path(),
            "kandinsky5-image-lite.safetensors",
            serde_json::json!({
                "model.diffusion_model.visual_embeddings.in_layer.bias": t.clone(),
                "model.diffusion_model.text_transformer_blocks.0.feed_forward.in_layer.weight": t,
            }),
        );
        assert_eq!(
            detect("diffusion_models", &path, None).as_deref(),
            Some("kandinsky5-image")
        );
        let recipe = image_recipe("kandinsky5_image");
        assert_eq!(recipe.latent, Some("EmptySD3LatentImage"));
        assert_eq!((recipe.zero_negative, recipe.patch), (false, None));
    }

    #[test]
    fn sd15_signature_is_kept_below_sdxl() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let sd15 = write(
            dir.path(),
            "v1-5-pruned.safetensors",
            serde_json::json!({
                "model.diffusion_model.conditioner.embedders.0.transformer.text_model.embeddings.token_embedding.weight": t.clone(),
            }),
        );
        assert_eq!(detect("checkpoints", &sd15, None).as_deref(), Some("sd15"));
        // SDXL keeps its two-embedder signature, so the CLIP-L-only rule never wins there.
        let sdxl = write(
            dir.path(),
            "sd_xl_base_1.0.safetensors",
            serde_json::json!({
                "model.diffusion_model.conditioner.embedders.0.transformer.text_model.embeddings.token_embedding.weight": t.clone(),
                "model.diffusion_model.conditioner.embedders.1.transformer.text_model.embeddings.token_embedding.weight": t,
            }),
        );
        assert_eq!(detect("checkpoints", &sdxl, None).as_deref(), Some("sdxl"));
    }

    #[test]
    fn pixart_and_hunyuan_dit_signatures_keep_the_plain_recipe() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let pixart = write(
            dir.path(),
            "pixart_sigma.safetensors",
            serde_json::json!({
                "model.diffusion_model.y_embedder.y_embedding": t.clone(),
                "model.diffusion_model.adaln_single.linear.weight": t.clone(),
            }),
        );
        assert_eq!(
            detect("diffusion_models", &pixart, None).as_deref(),
            Some("pixart")
        );
        let hydit = write(
            dir.path(),
            "hunyuan_dit_1.5.safetensors",
            serde_json::json!({
                "model.diffusion_model.mlp_t5.0.weight": t.clone(),
                "model.diffusion_model.x_embedder.proj.weight": t,
            }),
        );
        assert_eq!(
            detect("diffusion_models", &hydit, None).as_deref(),
            Some("hunyuan-dit")
        );
        for id in ["pixart", "hunyuan-dit"] {
            let recipe = image_recipe_by_id(id).unwrap();
            assert_eq!(
                (recipe.latent, recipe.zero_negative, recipe.patch),
                (None, false, None)
            );
        }
    }

    #[test]
    fn hidream_signature_selects_the_family_without_a_sampling_patch() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let path = write(
            dir.path(),
            "hidream_i1_full.safetensors",
            serde_json::json!({
                "model.diffusion_model.caption_projection.0.linear.weight": t,
            }),
        );
        assert_eq!(
            detect("diffusion_models", &path, None).as_deref(),
            Some("hidream")
        );
        let recipe = image_recipe("hidream");
        assert_eq!(recipe.latent, Some("EmptySD3LatentImage"));
        assert_eq!((recipe.zero_negative, recipe.patch), (false, None));
    }

    #[test]
    fn z_image_signature_declares_its_recipe_points() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let dim = |n: u64| serde_json::json!({"dtype":"BF16","shape":[n,16],"data_offsets":[0,2]});
        let path = write(
            dir.path(),
            "z_image_turbo_bf16.safetensors",
            serde_json::json!({
                "model.diffusion_model.cap_embedder.1.weight": dim(3840),
                "model.diffusion_model.noise_refiner.0.attention.k_norm.weight": t.clone(),
            }),
        );
        assert_eq!(
            detect("diffusion_models", &path, None).as_deref(),
            Some("z-image")
        );
        // Lumina2 shares the key set; the 2304 shape selects it, not Z-Image.
        let lumina = write(
            dir.path(),
            "NetaYumev35_pretrained_all_in_one.safetensors",
            serde_json::json!({
                "model.diffusion_model.cap_embedder.1.weight": dim(2304),
                "model.diffusion_model.noise_refiner.0.attention.k_norm.weight": t.clone(),
            }),
        );
        assert_eq!(
            detect("checkpoints", &lumina, None).as_deref(),
            Some("lumina2")
        );
        let recipe = image_recipe("lumina2");
        assert_eq!(recipe.latent, Some("EmptySD3LatentImage"));
        assert!(recipe.zero_negative);
        assert_eq!(recipe.patch, Some(("ModelSamplingAuraFlow", 3.0)));
        // Unknown clip types keep the plain graph.
        let plain = image_recipe("unknown_type");
        assert_eq!(
            (plain.latent, plain.zero_negative, plain.patch),
            (None, false, None)
        );
    }

    #[test]
    fn detects_by_architecture_signature_and_base() {
        let dir = tempfile::tempdir().unwrap();
        let t = serde_json::json!({"dtype":"BF16","shape":[1],"data_offsets":[0,2]});
        let krea = write(
            dir.path(),
            "k.safetensors",
            serde_json::json!({"model.diffusion_model.txtfusion.projector.weight":t,"model.diffusion_model.blocks.0.attn.gate.weight":t}),
        );
        assert_eq!(
            detect("diffusion_models", &krea, None).as_deref(),
            Some("krea-2")
        );
        // Kohya spelling (`_` instead of `.`) of an Anima LoRA.
        let anima = write(
            dir.path(),
            "a.safetensors",
            serde_json::json!({"lora_unet_blocks_27_cross_attn_output_proj.lora_up.weight":t}),
        );
        assert_eq!(detect("loras", &anima, None).as_deref(), Some("anima"));
        let declared = write(
            dir.path(),
            "d.safetensors",
            serde_json::json!({"__metadata__":{"modelspec.architecture":"MiniMax-H3/lora"},"x":t}),
        );
        assert_eq!(
            detect("loras", &declared, None).as_deref(),
            Some("minimax-h3")
        );
        // Turbo shares the base header; only the file name tells them apart, and LoRAs stay with the base.
        let q =
            serde_json::json!({"__metadata__":{"modelspec.architecture":"qwen-image-2.1"},"x":t});
        let turbo = write(
            dir.path(),
            "qwen21TurboOfficial_bf16.safetensors",
            q.clone(),
        );
        assert_eq!(
            detect("diffusion_models", &turbo, None).as_deref(),
            Some("qwen-image-2.1-turbo")
        );
        let base = write(dir.path(), "qwen_image_2.1_int8.safetensors", q.clone());
        assert_eq!(
            detect("diffusion_models", &base, None).as_deref(),
            Some("qwen-image-2.1")
        );
        let lora = write(dir.path(), "turbo-style.safetensors", q);
        assert_eq!(
            detect("loras", &lora, None).as_deref(),
            Some("qwen-image-2.1")
        );
        let unknown = write(dir.path(), "u.safetensors", serde_json::json!({"x":t}));
        // "turbo" alone never makes a Qwen Turbo: the header (or CivitAI base) must match too.
        let sdxl_turbo = write(
            dir.path(),
            "sdxl_turbo.safetensors",
            serde_json::json!({"conditioner.embedders.1.model.ln_final.weight":t}),
        );
        assert_eq!(
            detect("checkpoints", &sdxl_turbo, None).as_deref(),
            Some("sdxl")
        );
        let odd_turbo = write(dir.path(), "turbo.safetensors", serde_json::json!({"x":t}));
        assert_eq!(
            detect("checkpoints", &odd_turbo, Some("SDXL Turbo")).as_deref(),
            Some("sdxl")
        );
        assert_eq!(detect("checkpoints", &odd_turbo, None), None);
        assert_eq!(detect("checkpoints", &unknown, None), None);
        assert_eq!(
            detect("checkpoints", &unknown, Some("illustrious")).as_deref(),
            Some("sdxl")
        );
        assert_eq!(detect("vae", &krea, None), None);
        let huge = dir.path().join("h.safetensors");
        std::fs::write(&huge, (MAX_HEADER + 1).to_le_bytes()).unwrap();
        assert_eq!(detect("checkpoints", &huge, None), None);
    }

    /// Minimal GGUF v3 writer: a few KV pairs then tensor names, so detection sees preserved keys.
    fn write_gguf(dir: &Path, name: &str, arch: &str, keys: &[&str]) -> std::path::PathBuf {
        fn s(bytes: &mut Vec<u8>, v: &str) {
            bytes.extend((v.len() as u64).to_le_bytes());
            bytes.extend(v.as_bytes());
        }
        let mut bytes = b"GGUF".to_vec();
        bytes.extend(3u32.to_le_bytes()); // version
        bytes.extend((keys.len() as u64).to_le_bytes()); // tensor count
        bytes.extend((1u64).to_le_bytes()); // KV count
        s(&mut bytes, "general.architecture");
        bytes.extend(8u32.to_le_bytes()); // STRING
        s(&mut bytes, arch);
        for key in keys {
            s(&mut bytes, key);
            bytes.extend(2u32.to_le_bytes()); // n_dims
            bytes.extend([2u64, 4].map(|d| d.to_le_bytes()).concat()); // dims
            bytes.extend(0u32.to_le_bytes()); // GGML F32
            bytes.extend(0u64.to_le_bytes()); // offset
        }
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn detects_gguf_by_preserved_tensor_names_and_arch() {
        let dir = tempfile::tempdir().unwrap();
        let krea = write_gguf(
            dir.path(),
            "k.gguf",
            "krea",
            &[
                "model.diffusion_model.txtfusion.projector.weight",
                "model.diffusion_model.blocks.0.attn.gate.weight",
            ],
        );
        assert_eq!(
            detect("diffusion_models", &krea, None).as_deref(),
            Some("krea-2")
        );
        // A GGUF whose tensor names match nothing stays unknown, never a filename guess.
        let odd = write_gguf(dir.path(), "flux.gguf", "weird", &["a.b.weight"]);
        assert_eq!(detect("diffusion_models", &odd, None), None);
        // Non-safetensors companion architectures stay unknown for GGUF inputs.
        assert_eq!(detect("vae", &krea, None), None);
    }

    #[test]
    fn moss_identity_requires_both_model_signature_keys() {
        let dir = tempfile::tempdir().unwrap();
        let keys = [
            "shared_block_modulation.weight",
            "layers.0.proj.ffn_up_gate.weight",
        ];
        let both = serde_json::json!({keys[0]:{}, keys[1]:{}});
        let path = write(dir.path(), "renamed.safetensors", both.clone());
        assert_eq!(
            detect("diffusion_models", &path, None).as_deref(),
            Some("moss-image")
        );
        assert_eq!(detect("loras", &path, None), None);
        for key in keys {
            let mut partial = both.clone();
            partial.as_object_mut().unwrap().remove(key);
            let path = write(dir.path(), "mossImage_sft_bf16.safetensors", partial);
            assert_eq!(detect("diffusion_models", &path, None), None);
        }
    }

    type Signature<'a> = &'a [(&'a str, &'a [u64])];

    #[test]
    fn yue2_checkpoint_identity_requires_both_header_keys() {
        // Keys and shapes as inspected header-only on a real YuE2 3B checkpoint; no payload is read.
        let dir = tempfile::tempdir().unwrap();
        let keys = [
            "model.diffusion_model.llm2vae.weight",
            "text_encoders.model.embed_tokens.weight",
        ];
        let both = serde_json::json!({keys[0]:{"dtype":"BF16","shape":[64,2048]}, keys[1]:{"dtype":"BF16","shape":[184704,2048]}});
        let path = write(dir.path(), "renamed.safetensors", both.clone());
        assert_eq!(detect("checkpoints", &path, None).as_deref(), Some("yue2"));
        assert_eq!(detect("loras", &path, None), None);
        for key in keys {
            let mut partial = both.clone();
            partial.as_object_mut().unwrap().remove(key);
            let path = write(dir.path(), "yue2_3b.safetensors", partial);
            assert_ne!(detect("checkpoints", &path, None).as_deref(), Some("yue2"));
        }
    }

    // Sparse payloads exercise real header/offset validation without allocating model weights.
    fn component_file(dir: &Path, name: &str, tensors: Signature<'_>) -> std::path::PathBuf {
        let mut head = serde_json::Map::new();
        let mut end = 0;
        for (key, shape) in tensors {
            let start = end;
            end += shape.iter().product::<u64>() * 2;
            head.insert(
                (*key).into(),
                serde_json::json!({"dtype":"BF16", "shape":shape, "data_offsets":[start,end]}),
            );
        }
        let path = write(dir, name, Value::Object(head));
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(file.metadata().unwrap().len() + end).unwrap();
        path
    }

    #[test]
    fn moss_companions_identify_unpacked_architectures_not_packed_weights() {
        let dir = tempfile::tempdir().unwrap();
        let qwen: Signature<'_> = &[
            ("model.language_model.embed_tokens.weight", &[248320, 4096]),
            (
                "model.language_model.layers.0.input_layernorm.weight",
                &[4096],
            ),
            ("model.language_model.layers.0.linear_attn.A_log", &[32]),
            (
                "model.language_model.layers.0.linear_attn.in_proj_qkv.weight",
                &[8192, 4096],
            ),
            (
                "model.language_model.layers.31.self_attn.q_proj.weight",
                &[8192, 4096],
            ),
            (
                "model.language_model.layers.31.mlp.up_proj.weight",
                &[12288, 4096],
            ),
        ];
        let vae: Signature<'_> = &[
            ("encoder.conv_in.weight", &[128, 3, 3, 3]),
            ("decoder.conv_in.weight", &[512, 32, 3, 3]),
            ("decoder.conv_out.weight", &[3, 128, 3, 3]),
            ("encoder.quant_conv.weight", &[64, 64, 1, 1]),
            ("bn.running_mean", &[128]),
            ("bn.running_var", &[128]),
        ];
        for (category, id, signature) in [("clip", "qwen3.5-9b", qwen), ("vae", "flux2-vae", vae)] {
            let path = component_file(dir.path(), "renamed.safetensors", signature);
            assert_eq!(detect_component(category, &path), Some(id));
            for n in 0..signature.len() {
                let mut wrong = signature.to_vec();
                wrong[n].1 = &[1];
                let path = component_file(dir.path(), "mossImage_sft_txt_int4.safetensors", &wrong);
                assert_eq!(
                    detect_component(category, &path),
                    None,
                    "{}",
                    signature[n].0
                );
            }
        }
        let mut packed = qwen.to_vec();
        packed[3].1 = &[8192, 2048];
        packed[4].1 = &[8192, 2048];
        packed[5].1 = &[12288, 2048];
        let path = component_file(dir.path(), "qwen3.5_9b.safetensors", &packed);
        assert_eq!(detect_component("text_encoders", &path), None);
    }

    const QWEN_06: &[(&str, &[u64])] = &[
        ("model.embed_tokens.weight", &[151936, 1024]),
        ("model.layers.0.post_attention_layernorm.weight", &[1024]),
        ("model.layers.0.self_attn.q_norm.weight", &[128]),
        ("model.layers.0.self_attn.q_proj.weight", &[2048, 1024]),
        ("model.layers.27.mlp.up_proj.weight", &[3072, 1024]),
    ];

    #[test]
    fn companion_detection_ignores_renaming_and_category_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let path = component_file(dir.path(), "qwen_3_600m.safetensors", QWEN_06);
        assert_eq!(detect_component("text_encoders", &path), Some("qwen3-0.6b"));
        let renamed = dir
            .path()
            .join("dasiwaAnima_luminousLabyrinthV1_txt.safetensors");
        std::fs::rename(path, &renamed).unwrap();
        assert_eq!(detect_component("clip", &renamed), Some("qwen3-0.6b"));
        assert_eq!(detect_component("vae", &renamed), None);
        assert_eq!(detect_component("loras", &renamed), None);
        assert_eq!(detect("text_encoders", &renamed, Some("Anima")), None);
        let wrong_format = renamed.with_extension("pt");
        std::fs::rename(renamed, &wrong_format).unwrap();
        assert_eq!(detect_component("clip", &wrong_format), None);
    }

    #[test]
    fn companion_detection_rejects_wrong_shapes_unknown_and_malformed_headers() {
        let dir = tempfile::tempdir().unwrap();
        for (key, _) in QWEN_06 {
            let mut tensors = QWEN_06.to_vec();
            tensors.iter_mut().find(|(k, _)| k == key).unwrap().1 = &[1];
            let path = component_file(dir.path(), "qwen_3_600m.safetensors", &tensors);
            assert_eq!(detect_component("text_encoders", &path), None, "{key}");
        }
        let path = component_file(dir.path(), "unknown.sft", &[("random.weight", &[1024])]);
        assert_eq!(detect_component("text_encoders", &path), None);
        let path = component_file(dir.path(), "truncated.safetensors", QWEN_06);
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(file.metadata().unwrap().len() - 1).unwrap();
        assert_eq!(detect_component("text_encoders", &path), None);
        for head in [
            serde_json::json!({"x":{"shape":[1],"dtype":"BF16","data_offsets":[2,0]}}),
            serde_json::json!({"x":{"shape":[u64::MAX,2],"dtype":"BF16","data_offsets":[0,0]}}),
            serde_json::json!({"x":{"shape":[-1],"dtype":"BF16","data_offsets":[0,0]}}),
            serde_json::json!({"x":{"shape":[1],"dtype":"BAD","data_offsets":[0,2]}}),
            serde_json::json!({"model.layers.0.self_attn.q_norm.weight":{}}),
        ] {
            let path = write(dir.path(), "bad.safetensors", head);
            assert_eq!(detect_component("text_encoders", &path), None);
        }
        for bytes in [
            vec![],
            vec![0; 7],
            (MAX_HEADER + 1).to_le_bytes().to_vec(),
            [8u64.to_le_bytes().as_slice(), b"{"].concat(),
        ] {
            let path = dir.path().join("bad.sft");
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(detect_component("clip", &path), None);
        }
    }

    #[test]
    fn companion_tensor_validation_rejects_bad_descriptors_and_overlaps() {
        let tensor = serde_json::json!({"dtype":"BF16","shape":[2],"data_offsets":[0,4]});
        let valid = serde_json::json!({"a":tensor});
        assert_eq!(valid_tensors(valid.as_object().unwrap(), 4), Some(()));
        for (field, bad) in [
            ("shape", serde_json::json!([-1])),
            ("shape", serde_json::json!([u64::MAX, 2])),
            ("shape", serde_json::json!([1, 1, 1, 1, 1, 1, 1, 1, 1])),
            ("dtype", serde_json::json!("unknown")),
            ("data_offsets", serde_json::json!([4, 0])),
            ("data_offsets", serde_json::json!([0, 2])),
            ("data_offsets", serde_json::json!([0, 4, 4])),
            ("data_offsets", serde_json::json!([4, 8])),
        ] {
            let mut head = valid.clone();
            head["a"][field] = bad;
            assert_eq!(valid_tensors(head.as_object().unwrap(), 4), None, "{field}");
        }
        for bad in [serde_json::json!(0), serde_json::json!({"config":7})] {
            let mut head = valid.clone();
            head["__metadata__"] = bad;
            assert_eq!(valid_tensors(head.as_object().unwrap(), 4), None);
        }
        let overlap = serde_json::json!({"a":tensor,"b":tensor});
        assert_eq!(valid_tensors(overlap.as_object().unwrap(), 4), None);
        assert_eq!(valid_tensors(valid.as_object().unwrap(), 5), None);
    }

    #[test]
    fn companion_signatures_discriminate_architectures_and_export_prefixes() {
        let dir = tempfile::tempdir().unwrap();
        for prefix in ["model.", "model.language_model."] {
            for (id, hidden, mlp, visual) in [
                ("qwen3-vl-4b", 2560, 9728, 4096),
                ("qwen3-vl-8b", 4096, 12288, 4608),
            ] {
                let keys = [
                    "embed_tokens.weight",
                    "layers.0.post_attention_layernorm.weight",
                    "layers.0.self_attn.q_norm.weight",
                    "layers.0.self_attn.q_proj.weight",
                    "layers.35.mlp.up_proj.weight",
                ]
                .map(|k| format!("{prefix}{k}"));
                let dims = [
                    vec![151936, hidden],
                    vec![hidden],
                    vec![128],
                    vec![4096, hidden],
                    vec![mlp, hidden],
                ];
                let mut tensors: Vec<_> = keys
                    .iter()
                    .zip(&dims)
                    .map(|(k, s)| (k.as_str(), s.as_slice()))
                    .collect();
                let norm = [visual];
                let merger = [hidden, visual];
                tensors.push(("model.visual.deepstack_merger_list.0.norm.weight", &norm));
                tensors.push(("model.visual.merger.linear_fc2.weight", &merger));
                let path = component_file(dir.path(), "renamed.sft", &tensors);
                assert_eq!(detect_component("text_encoders", &path), Some(id));
                tensors.pop();
                let path = component_file(dir.path(), "qwen3vl_4b.safetensors", &tensors);
                assert_eq!(detect_component("text_encoders", &path), None);
            }
        }
        let cases: &[(&str, &str, Signature<'_>)] = &[
            (
                "vae",
                "wan-2.1-vae",
                &[
                    ("conv2.weight", &[16, 16, 1, 1, 1]),
                    ("encoder.conv1.weight", &[96, 3, 3, 3, 3]),
                    ("decoder.conv1.weight", &[384, 16, 3, 3, 3]),
                    ("decoder.middle.0.residual.0.gamma", &[384, 1, 1, 1]),
                    ("decoder.head.2.weight", &[3, 96, 3, 3, 3]),
                ],
            ),
            (
                "vae",
                "qwen-image-2.1-vae",
                &[
                    ("conv2.weight", &[64, 64, 1, 1, 1]),
                    ("encoder.conv1.weight", &[96, 4, 1, 3, 3]),
                    ("decoder.conv1.weight", &[1152, 64, 1, 3, 3]),
                    ("decoder.middle.0.residual.0.gamma", &[1152, 1, 1, 1]),
                    ("decoder.head.2.weight", &[4, 144, 1, 3, 3]),
                    ("decoder.upsamples.0.upsamples.0.residual.2.weight", &[1]),
                ],
            ),
            (
                "text_encoders",
                "umt5-xxl",
                &[
                    ("shared.weight", &[256384, 4096]),
                    (
                        "encoder.block.0.layer.0.SelfAttention.k.weight",
                        &[4096, 4096],
                    ),
                    (
                        "encoder.block.23.layer.1.DenseReluDense.wi_1.weight",
                        &[10240, 4096],
                    ),
                ],
            ),
            (
                "vae",
                "ltx-2.3-audio-vae",
                &[
                    ("audio_vae.encoder.conv_in.conv.weight", &[128, 2, 3, 3]),
                    ("audio_vae.decoder.conv_in.conv.weight", &[512, 8, 3, 3]),
                    ("audio_vae.decoder.conv_out.conv.weight", &[2, 128, 3, 3]),
                    ("audio_vae.per_channel_statistics.mean-of-means", &[128]),
                    (
                        "vocoder.vocoder.resblocks.0.convs1.0.weight",
                        &[768, 768, 3],
                    ),
                ],
            ),
            (
                "text_encoders",
                "ltx-2.3-text-projection",
                &[
                    (
                        "text_embedding_projection.audio_aggregate_embed.weight",
                        &[2048, 188160],
                    ),
                    (
                        "text_embedding_projection.audio_aggregate_embed.bias",
                        &[2048],
                    ),
                    (
                        "text_embedding_projection.video_aggregate_embed.weight",
                        &[4096, 188160],
                    ),
                    (
                        "text_embedding_projection.video_aggregate_embed.bias",
                        &[4096],
                    ),
                ],
            ),
        ];
        for (category, id, tensors) in cases {
            let path = component_file(dir.path(), "unrelated-name.safetensors", tensors);
            assert_eq!(detect_component(category, &path), Some(*id));
            assert_eq!(
                detect_component(if *category == "vae" { "clip" } else { "vae" }, &path),
                None
            );
            for i in 0..tensors.len() {
                let mut wrong = tensors.to_vec();
                wrong[i].1 = &[2];
                let path = component_file(dir.path(), "familiar-name.safetensors", &wrong);
                // The Wan 2.2 layout marker is a presence guard, not a learned weight signature.
                if !wrong[i].0.contains("upsamples.0.upsamples") {
                    assert_eq!(
                        detect_component(category, &path),
                        None,
                        "{id}: {}",
                        wrong[i].0
                    );
                }
            }
        }
    }

    /// `DEAS_LIVE_MODELS=<models root> cargo test --lib live_family_scan -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_family_scan() {
        let root = std::env::var("DEAS_LIVE_MODELS").expect("DEAS_LIVE_MODELS");
        let started = std::time::Instant::now();
        let models = crate::downloads::models::scan(Path::new(&root)).unwrap();
        for m in models.iter().filter(|m| {
            [
                "checkpoints",
                "diffusion_models",
                "unet",
                "loras",
                "text_encoders",
                "clip",
                "vae",
            ]
            .contains(&m.category.as_str())
        }) {
            println!(
                "{:<17} {:<28} {}",
                m.category,
                m.component
                    .as_deref()
                    .or(m.family.as_deref())
                    .unwrap_or("-"),
                m.name
            );
        }
        println!("{} files in {:?}", models.len(), started.elapsed());
    }
}
