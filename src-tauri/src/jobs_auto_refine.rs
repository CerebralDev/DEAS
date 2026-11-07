//! Owned, serial SAM → cropped inpaint orchestration; every pass remains a Workbench job.
use super::*;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoRefineSettings {
    pub checkpoint: String,
    pub targets: Vec<String>,
    pub denoise: f64,
    pub margin: u32,
    pub threshold: f64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefineMask {
    pub name: String,
    pub png: Vec<u8>,
}

impl AutoRefineSettings {
    fn validate(&self, masks: &[RefineMask]) -> Result<(), String> {
        if (!self.targets.is_empty() && !comfy_model_name(&self.checkpoint))
            || !self.denoise.is_finite()
            || self.denoise <= 0.0
            || self.denoise > 1.0
            || !self.threshold.is_finite()
            || !(0.0..=1.0).contains(&self.threshold)
            || self.margin > 8192
            || self.targets.len() + masks.len() > 20
            || self.targets.is_empty() && masks.is_empty()
            || self.targets.iter().any(|s| !valid_label(s))
            || masks.iter().any(|m| !valid_label(&m.name))
        {
            return Err("Invalid Auto Refine settings or mask names".into());
        }
        Ok(())
    }
}

fn valid_label(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}

struct Pixels {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

fn decode_png(bytes: &[u8]) -> Result<Pixels, String> {
    if bytes.len() > 64 << 20 {
        return Err("Auto Refine PNG exceeds 64 MiB".into());
    }
    validate_png_envelope(bytes)?;
    let mut options = png::DecodeOptions::default();
    options.set_ignore_crc(false);
    options.set_ignore_adler32(false);
    options.set_skip_ancillary_crc_failures(false);
    options.set_ignore_text_chunk(true);
    options.set_ignore_iccp_chunk(true);
    let mut decoder = png::Decoder::new_with_options(std::io::Cursor::new(bytes), options);
    decoder.set_limits(png::Limits { bytes: 256 << 20 });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|_| "Invalid Auto Refine PNG header")?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16_777_216
        || reader.info().animation_control.is_some()
        || reader.output_buffer_size() > 256 << 20
    {
        return Err("Invalid Auto Refine PNG dimensions or animation".into());
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|_| "Invalid Auto Refine PNG frame")?;
    reader.finish().map_err(|_| "Invalid Auto Refine PNG end")?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err("Auto Refine requires decoded 8-bit PNG pixels".into());
    }
    let channels = info.color_type.samples();
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for p in buffer[..info.buffer_size()].chunks_exact(channels) {
        match info.color_type {
            png::ColorType::Rgb | png::ColorType::Rgba => rgb.extend_from_slice(&p[..3]),
            png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha => {
                rgb.extend_from_slice(&[p[0]; 3])
            }
            _ => return Err("Unsupported Auto Refine PNG pixels".into()),
        }
    }
    Ok(Pixels { width, height, rgb })
}

fn encode_png(pixels: &Pixels) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, pixels.width, pixels.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| "Auto Refine PNG encoding failed")?;
        writer
            .write_image_data(&pixels.rgb)
            .map_err(|_| "Auto Refine PNG encoding failed")?;
        writer
            .finish()
            .map_err(|_| "Auto Refine PNG encoding failed")?;
    }
    Ok(bytes)
}

/// Red is the existing LoadImageMask channel; alpha is deliberately not mask authority.
fn mask_bounds(pixels: &Pixels, margin: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut left, mut top, mut right, mut bottom) = (pixels.width, pixels.height, 0, 0);
    for (index, pixel) in pixels.rgb.chunks_exact(3).enumerate() {
        if pixel[0] > 0 {
            let x = index as u32 % pixels.width;
            let y = index as u32 / pixels.width;
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if right == 0 {
        return None;
    }
    left = left.saturating_sub(margin);
    top = top.saturating_sub(margin);
    right = right.saturating_add(margin).min(pixels.width);
    bottom = bottom.saturating_add(margin).min(pixels.height);
    Some((left, top, right - left, bottom - top))
}

fn crop_target(width: u32, height: u32, area: u64, grid: u64) -> Result<(u64, u64), String> {
    // Same area-exact custom-aspect math and final grid snap as lib/resolution.ts canvasSize.
    let ratio = f64::from(width) / f64::from(height);
    let snap = |n: f64| ((n / grid as f64).round() as u64 * grid).max(64_u64.div_ceil(grid) * grid);
    let w = snap((area as f64 * ratio).sqrt());
    let h = snap((area as f64 / ratio).sqrt());
    if w > 8192 || h > 8192 || w * h > 16_777_216 {
        return Err("Auto Refine crop target exceeds the native canvas limits".into());
    }
    Ok((w, h))
}

fn model_canvas(intent: &Value) -> (u64, u64) {
    let families: Vec<Value> =
        serde_json::from_str(include_str!("../../src/families/builtin.json"))
            .expect("bundled family plugin is valid");
    let id = intent["family"]
        .as_str()
        .or_else(|| intent["diffusion"]["family"].as_str());
    let family = families.iter().find(|f| {
        id.is_some_and(|id| f["id"] == id)
            || id.is_none()
                && intent["diffusion"]["clipType"].is_string()
                && f["clipType"] == intent["diffusion"]["clipType"]
    });
    family.map_or((1024 * 1024, 16), |f| {
        (
            f["defaults"]["width"].as_u64().unwrap_or(1024)
                * f["defaults"]["height"].as_u64().unwrap_or(1024),
            if intent["nativeImageReference"]["method"] == "qwen-image-2.1" {
                32
            } else {
                f["multiple"].as_u64().unwrap_or(16)
            },
        )
    })
}

fn pass_intent(
    base: &Value,
    source: &str,
    mask: &str,
    size: (u32, u32),
    crop: Value,
    denoise: f64,
    chained: bool,
) -> Result<Value, String> {
    let mut intent = base.clone();
    let object = intent
        .as_object_mut()
        .ok_or("Auto Refine requires an image intent")?;
    for key in [
        "inputResize",
        "inputLayers",
        "inputMask",
        "inpaintCrop",
        "maskBlur",
        "refine",
        "upscaleModel",
        "videoPost",
        "audioEnhance",
        "latentUpscale",
        "autoRefine",
    ] {
        object.remove(key);
    }
    object.insert("inputImage".into(), json!(source));
    object.insert("inputMask".into(), json!(mask));
    object.insert("width".into(), json!(size.0));
    object.insert("height".into(), json!(size.1));
    object.insert("denoise".into(), json!(denoise));
    object.insert("inpaintCrop".into(), crop);
    // shortcut: chained passes are read back for the next pass, so only PNG survives that
    // transport; the final executed pass keeps the requested export format. If the last tasks
    // all skip, the final product stays PNG.
    if chained {
        object.insert("imageFormat".into(), json!("png"));
    }
    // Fixed schedules are already literal: use the same round(steps*denoise) tail as Workbench.
    // Ordinary samplers retain the exact generation steps; never substitute a reduced pass default.
    if let Some(sigmas) = object.get("sigmas").and_then(Value::as_array) {
        if sigmas.len() < 2 {
            return Err("Invalid Auto Refine fixed sigma schedule".into());
        }
        let count = (((sigmas.len() - 1) as f64 * denoise).round() as usize).max(1);
        let tail = sigmas[sigmas.len() - count - 1..].to_vec();
        object.insert("steps".into(), json!(count));
        object.insert("sigmas".into(), json!(tail));
    }
    // Standard masked inpaint always composites onto the untouched full-size input.
    let graph = compile(&intent)?;
    if graph["5"]["class_type"] == "SamplerCustomAdvanced"
        && intent.get("sigmas").is_none()
        && denoise < 1.0
        && graph["ZD"]["class_type"] != "SplitSigmas"
    {
        return Err("Auto Refine custom scheduler has no denoise support".into());
    }
    Ok(intent)
}

fn output_index(job: &Value) -> Result<usize, String> {
    let outputs = job["outputs"]
        .as_array()
        .ok_or("Auto Refine source has no outputs")?;
    outputs
        .iter()
        .rposition(|o| o["role"] == "processed")
        .or_else(|| outputs.len().checked_sub(1))
        .ok_or("Auto Refine source has no outputs".into())
}

fn log(operation: &str, id: Option<&str>, error: bool) {
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!(
            "{}: origin=auto-refine operation={operation} job={}",
            if error { "ERROR" } else { "INFO" },
            id.unwrap_or("none")
        ),
    );
}

struct Operation(Jobs);
impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut session) = self.0.lock() {
            session.auto_refine = false;
        }
    }
}

impl Jobs {
    fn refine_fence(&self, epoch: u64) -> Result<(), String> {
        let session = self.lock()?;
        if session.admission_epoch != epoch || session.stop.is_some() {
            return Err("Auto Refine cancelled by Workbench Stop; no further job submitted".into());
        }
        Ok(())
    }

    async fn refine_wait(&self, held: &Jobs, id: &str, epoch: u64) -> Result<Value, String> {
        loop {
            self.refine_fence(epoch)?;
            let current = held.current()?;
            match current["state"].as_str() {
                Some("succeeded") => return Ok(current),
                Some("cancelled") => {
                    log("cancelled", Some(id), false);
                    return Err("Auto Refine cancelled; no further job submitted".into());
                }
                Some("failed" | "unknown") | None => {
                    log("await-terminal", Some(id), true);
                    return Err(
                        "Auto Refine job failed, cancelled or is unknown; no retry submitted"
                            .into(),
                    );
                }
                Some("queued" | "running") => {}
                _ => return Err("Auto Refine encountered an unsupported job state".into()),
            }
            held.status_for(id.to_owned()).await?;
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    async fn refine_run(
        &self,
        engine: &crate::install::Engine,
        base: &str,
        intent: Value,
        epoch: u64,
    ) -> Result<(Arc<Jobs>, Value), String> {
        self.refine_fence(epoch)?;
        let job = self
            .workbench_auto_refine_run(base.to_owned(), intent, epoch, engine)
            .await?;
        let id = job["id"]
            .as_str()
            .ok_or("Auto Refine job identity missing")?;
        let held = self
            .workbench_job(id)?
            .ok_or("Auto Refine job no longer held")?;
        let terminal = self.refine_wait(&held, id, epoch).await?;
        Ok((held, terminal))
    }

    async fn refine_output(&self, held: &Jobs, job: &Value, epoch: u64) -> Result<Pixels, String> {
        self.refine_fence(epoch)?;
        let id = job["id"]
            .as_str()
            .ok_or("Auto Refine source job identity missing")?;
        // Reuse the native output authority, transport limits, PNG validation and output cache.
        let preview = held.output(id.to_owned(), output_index(job)?).await?;
        decode_png(&preview.bytes)
    }

    #[allow(clippy::too_many_arguments)]
    async fn refine_execute(
        &self,
        engine: crate::install::Engine,
        base_url: String,
        mut intent: Value,
        settings: AutoRefineSettings,
        source: Option<Arc<Jobs>>,
        masks: Vec<RefineMask>,
        epoch: u64,
    ) -> Result<Value, String> {
        let _operation = Operation(self.clone());
        let mut last = None;
        let manual = source.is_none();
        let mut current = if let Some(held) = source {
            let id = held.current()?["id"]
                .as_str()
                .ok_or("Auto Refine source identity missing")?
                .to_owned();
            let terminal = self.refine_wait(&held, &id, epoch).await?;
            // Authoritative immutable generation settings, not subsequently edited UI settings.
            intent = terminal["intent"].clone();
            if intent["imageFormat"].as_str().is_some_and(|f| f != "png") {
                return Err(
                    "Auto Refine needs the source job to save PNG output; switch the output format and rerun"
                        .into(),
                );
            }
            let pixels = self.refine_output(&held, &terminal, epoch).await?;
            last = Some(terminal);
            pixels
        } else {
            self.refine_fence(epoch)?;
            let image = intent["inputImage"]
                .as_str()
                .filter(|s| native_png_input(s))
                .ok_or("Manual Auto Refine requires a native uploaded PNG inputImage")?;
            fetch_input_png(&base_url, image).await?
        };
        if crate::jobs_video::is_video(&intent) || crate::jobs_music::is_music(&intent) {
            return Err("Auto Refine requires an image generation intent".into());
        }
        let size = (current.width, current.height);
        if manual && source_size(&intent) != Some((u64::from(size.0), u64::from(size.1))) {
            return Err("Auto Refine input dimensions do not match the verified source".into());
        }
        // Validate preserved model/settings before the first SAM job can run.
        let mut check = intent.clone();
        let check_object = check
            .as_object_mut()
            .ok_or("Auto Refine requires an image intent")?;
        for key in [
            "refine",
            "upscaleModel",
            "inputResize",
            "inputLayers",
            "inputMask",
            "inpaintCrop",
            "maskBlur",
            "autoRefine",
        ] {
            check_object.remove(key);
        }
        check_object.insert("width".into(), json!(size.0));
        check_object.insert("height".into(), json!(size.1));
        check_object.remove("inputImage");
        check_object.remove("denoise");
        // H3 is input-only, so its existing compiler validates it on the first cropped pass.
        if check["family"] != "minimax-h3" {
            compile(&check)?;
        }
        let (area, grid) = model_canvas(&intent);
        let mut skipped = 0;
        // Every explicit mask must fit the verified source before the first upload or job.
        for mask in &masks {
            let pixels = decode_png(&mask.png)?;
            if (pixels.width, pixels.height) != size {
                return Err("Auto Refine mask dimensions do not match the verified source".into());
            }
        }
        let tasks: Vec<Option<RefineMask>> = {
            let mut tasks: Vec<Option<RefineMask>> = masks.into_iter().map(Some).collect();
            tasks.extend(settings.targets.iter().map(|_| None));
            tasks
        };
        self.refine_slots_ok(tasks.len())?;
        let mut targets = settings.targets.iter();
        for (index, task) in tasks.iter().enumerate() {
            self.refine_fence(epoch)?;
            let source = native_mask_upload(base_url.clone(), encode_png(&current)?).await?;
            self.refine_fence(epoch)?;
            let mut mask = match task {
                Some(mask) => decode_png(&mask.png)?,
                None => {
                    let target = targets.next().ok_or("Auto Refine target missing")?;
                    let sam = json!({"recipeId":"sam3-mask","version":1,"checkpoint":settings.checkpoint,
                        "inputImage":source,"width":size.0,"height":size.1,
                        "outputPrefix":"raw/auto-refine-mask","prompt":target,"threshold":settings.threshold});
                    let (held, job) = self.refine_run(&engine, &base_url, sam, epoch).await?;
                    self.refine_output(&held, &job, epoch).await?
                }
            };
            if (mask.width, mask.height) != size {
                return Err("Auto Refine mask dimensions do not match the verified source".into());
            }
            for pixel in mask.rgb.chunks_exact_mut(3) {
                pixel[1] = pixel[0];
                pixel[2] = pixel[0];
            }
            let Some((x, y, width, height)) = mask_bounds(&mask, settings.margin) else {
                skipped += 1;
                log(
                    "empty-mask-skip",
                    last.as_ref().and_then(|j| j["id"].as_str()),
                    false,
                );
                continue;
            };
            let (tw, th) = crop_target(width, height, area, grid)?;
            self.refine_fence(epoch)?;
            let mask = native_mask_upload(base_url.clone(), encode_png(&mask)?).await?;
            let pass = pass_intent(
                &intent,
                &source,
                &mask,
                size,
                json!({"x":x,"y":y,"width":width,"height":height,"targetWidth":tw,"targetHeight":th}),
                settings.denoise,
                index + 1 < tasks.len(),
            )?;
            let (held, job) = self.refine_run(&engine, &base_url, pass, epoch).await?;
            let pixels = self.refine_output(&held, &job, epoch).await?;
            if (pixels.width, pixels.height) != size {
                return Err("Auto Refine changed the source canvas".into());
            }
            current = pixels;
            last = Some(job);
        }
        self.refine_fence(epoch)?;
        Ok(json!({"job":last,"skipped":skipped}))
    }
}

fn source_size(intent: &Value) -> Option<(u64, u64)> {
    Some((intent["width"].as_u64()?, intent["height"].as_u64()?))
}

fn native_png_input(name: &str) -> bool {
    mask_name(name)
        || name
            .strip_prefix("deas-inputs/")
            .and_then(|s| s.strip_suffix(".png"))
            .is_some_and(|s| s.len() == 36 && uuid::Uuid::parse_str(s).is_ok())
}

async fn fetch_input_png(base: &str, image: &str) -> Result<Pixels, String> {
    let mut url = crate::validate_base_url(base)?
        .join("view")
        .map_err(|_| "Invalid input endpoint")?;
    let (folder, filename) = image.rsplit_once('/').unwrap_or(("", image));
    url.query_pairs_mut()
        .append_pair("filename", filename)
        .append_pair("subfolder", folder)
        .append_pair("type", "input");
    let mut response = client()?
        .get(url)
        .send()
        .await
        .map_err(|_| "Auto Refine input request failed")?;
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > 64 << 20) {
        return Err("Auto Refine input response refused".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Auto Refine input response interrupted")?
    {
        if chunk.len() > (64 << 20) - bytes.len() {
            return Err("Auto Refine input exceeds PNG limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    decode_png(&bytes)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn native_auto_refine(
    state: tauri::State<'_, Jobs>,
    engine: tauri::State<'_, crate::install::Engine>,
    base_url: String,
    intent: Value,
    settings: AutoRefineSettings,
    source_job_id: Option<String>,
    masks: Vec<RefineMask>,
    confirmed: bool,
    admission_epoch: u64,
) -> Result<Value, String> {
    if !confirmed {
        return Err("Explicit Auto Refine confirmation is required".into());
    }
    settings.validate(&masks)?;
    crate::validate_base_url(&base_url)?;
    let source = source_job_id
        .as_deref()
        .map(|id| {
            state
                .workbench_job(id)
                .and_then(|held| held.ok_or("Unknown held Auto Refine source job".into()))
        })
        .transpose()?;
    if let Some(held) = &source {
        let held = held.lock()?;
        if held.job.as_ref().is_none_or(|job| {
            job.plan.base.as_str()
                != crate::validate_base_url(&base_url)
                    .ok()
                    .as_ref()
                    .map(|u| u.as_str())
                    .unwrap_or("")
        }) {
            return Err("Auto Refine source endpoint does not match".into());
        }
    }
    // Validate every explicit mask before any upload/submission, including the last in the list.
    for mask in &masks {
        decode_png(&mask.png)?;
    }
    {
        let mut session = state.lock()?;
        if session.auto_refine
            || session.stop.is_some()
            || session.admission_epoch != admission_epoch
            || admission_epoch > workbench::MAX_ADMISSION_EPOCH
            || session.preparing.is_some()
            || session.job.as_ref().is_some_and(|j| !j.terminal())
        {
            log("rejected-admission", None, true);
            return Err("Auto Refine admission is busy or stale".into());
        }
        session.auto_refine = true;
    }
    let jobs = state.inner().clone();
    let engine = engine.inner().clone();
    tokio::spawn(async move {
        let result = jobs
            .refine_execute(
                engine,
                base_url,
                intent,
                settings,
                source,
                masks,
                admission_epoch,
            )
            .await;
        if let Err(error) = &result {
            let cancelled = jobs.refine_fence(admission_epoch).is_err()
                || error.starts_with("Auto Refine cancelled");
            log(
                if cancelled {
                    "cancelled"
                } else {
                    "serial-refine-failed"
                },
                source_job_id.as_deref(),
                !cancelled,
            );
        }
        result
    })
    .await
    .map_err(|_| "Auto Refine owned task failed")?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses only an explicitly supplied isolated ComfyUI; never starts/stops services or downloads weights.
    /// DEAS_SAM_TEST_URL=http://127.0.0.1:56525 cargo test --lib auto_refine_live -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "requires explicit DEAS_SAM_TEST_URL, SAM3 + SDXL weights and astronaut.png input"]
    async fn auto_refine_live_serial_face_hands() {
        let base = std::env::var("DEAS_SAM_TEST_URL")
            .expect("Set DEAS_SAM_TEST_URL to an isolated test backend");
        let pixels = fetch_input_png(&base, "astronaut.png").await.unwrap();
        assert_eq!((pixels.width, pixels.height), (512, 512));
        let uploaded = native_mask_upload(base.clone(), encode_png(&pixels).unwrap())
            .await
            .unwrap();
        let intent = json!({"recipeId":"checkpoint-text-to-image-diagnostic","version":1,
            "checkpoint":"Illustrious/DasiwaIllustriousAnime_epitaphecstasy.safetensors",
            "family":"sdxl","positivePrompt":"portrait photograph of an astronaut, detailed face",
            "negativePrompt":"","seed":123,"width":512,"height":512,"steps":20,"cfg":5,
            "sampler":"euler","scheduler":"normal","outputPrefix":"raw/auto-refine-live",
            "inputImage":uploaded,"denoise":0.25});
        let settings = AutoRefineSettings {
            checkpoint: "SAM3/sam3.1_multiplex_fp16.safetensors".into(),
            targets: vec!["face".into(), "hands".into()],
            denoise: 0.25,
            margin: 32,
            threshold: 0.5,
        };
        let jobs = Jobs::default();
        jobs.lock().unwrap().auto_refine = true;
        let result = jobs
            .refine_execute(
                crate::install::Engine::default(),
                base,
                intent.clone(),
                settings,
                None,
                vec![],
                0,
            )
            .await
            .unwrap();
        assert!(
            result["job"].is_object(),
            "face must produce an inpaint output: {result}"
        );
        assert_eq!(result["job"]["state"], "succeeded");
        let snapshot = jobs.workbench_current().unwrap();
        let passes: Vec<_> = snapshot["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|j| j["intent"]["recipeId"] != "sam3-mask")
            .collect();
        assert!(!passes.is_empty());
        for pass in &passes {
            for key in [
                "checkpoint",
                "positivePrompt",
                "negativePrompt",
                "steps",
                "cfg",
                "sampler",
                "scheduler",
            ] {
                assert_eq!(pass["intent"][key], intent[key]);
            }
            assert_eq!(pass["intent"]["denoise"], 0.25);
            assert_eq!(pass["intent"]["width"], 512);
            assert_eq!(pass["intent"]["height"], 512);
            assert!(pass["intent"]["inputMask"].as_str().is_some_and(mask_name));
            assert!(pass["intent"]["inpaintCrop"].is_object());
        }
        if passes.len() == 2 {
            assert_ne!(
                passes[0]["intent"]["inputImage"],
                passes[1]["intent"]["inputImage"]
            );
        }
        println!(
            "auto-refine live: passes={} skipped={} final_job={}",
            passes.len(),
            result["skipped"],
            result["job"]["id"]
        );
        println!("{}", snapshot);
        assert!(!jobs.lock().unwrap().auto_refine);
        let source = jobs
            .workbench_job(result["job"]["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        jobs.lock().unwrap().auto_refine = true;
        let automatic = jobs
            .refine_execute(
                crate::install::Engine::default(),
                std::env::var("DEAS_SAM_TEST_URL").unwrap(),
                json!({"untrustedDraft":"must not replace source settings"}),
                AutoRefineSettings {
                    checkpoint: "SAM3/sam3.1_multiplex_fp16.safetensors".into(),
                    targets: vec!["face".into()],
                    denoise: 0.25,
                    margin: 32,
                    threshold: 0.5,
                },
                Some(source),
                vec![],
                0,
            )
            .await
            .unwrap();
        assert_eq!(automatic["job"]["state"], "succeeded");
        for key in [
            "checkpoint",
            "positivePrompt",
            "negativePrompt",
            "steps",
            "cfg",
            "sampler",
            "scheduler",
        ] {
            assert_eq!(automatic["job"]["intent"][key], intent[key]);
        }
        println!("auto-refine held-source branch: {}", automatic);
    }

    #[test]
    fn auto_refine_bounds_are_tight_clamped_and_empty_never_falls_back() {
        let mut p = Pixels {
            width: 100,
            height: 80,
            rgb: vec![0; 100 * 80 * 3],
        };
        assert_eq!(mask_bounds(&p, 32), None);
        p.rgb[(2 * 100 + 4) * 3] = 255;
        p.rgb[(60 * 100 + 90) * 3] = 255;
        assert_eq!(mask_bounds(&p, 0), Some((4, 2, 87, 59)));
        assert_eq!(mask_bounds(&p, 32), Some((0, 0, 100, 80)));
    }

    #[test]
    fn auto_refine_crop_resolution_uses_native_area_and_family_grid() {
        assert_eq!(crop_target(3, 4, 1024 * 1024, 64).unwrap(), (896, 1152));
        assert_eq!(crop_target(1, 1, 512 * 512, 8).unwrap(), (512, 512));
        assert!(crop_target(8192, 1, 1024 * 1024, 16).is_err());
        assert_eq!(model_canvas(&json!({"family":"sd15"})), (512 * 512, 8));
    }

    #[test]
    fn auto_refine_custom_schedule_keeps_steps_and_cropped_geometry() {
        let base = json!({"recipeId":"checkpoint-text-to-image-diagnostic","version":1,
            "positivePrompt":"portrait","negativePrompt":"","checkpoint":"flux.safetensors",
            "seed":0,"width":512,"height":512,"steps":20,"cfg":1,
            "sampler":"euler","scheduler":"normal","outputPrefix":"test",
            "diffusion":{"textEncoder":"qwen.safetensors","clipType":"flux2","vae":"ae.safetensors","family":"flux2"}});
        let mask = "deas-masks/00000000-0000-0000-0000-000000000001.png";
        let pass = pass_intent(
            &base,
            mask,
            mask,
            (512, 512),
            json!({"x":0,"y":0,"width":256,"height":256,"targetWidth":1024,"targetHeight":1024}),
            0.25,
            true,
        )
        .unwrap();
        assert_eq!(pass["steps"], 20);
        assert_eq!(pass["imageFormat"], "png");
        let graph = compile(&pass).unwrap();
        assert_eq!(
            graph["Z"]["inputs"],
            json!({"steps":80,"width":1024,"height":1024})
        );
        assert_eq!(
            graph["ZD"],
            json!({"class_type":"SplitSigmas","inputs":{"sigmas":["Z",0],"step":60}})
        );
        assert_eq!(graph["5"]["inputs"]["sigmas"], json!(["ZD", 1]));
        assert_eq!(model_canvas(&json!({"family":"qwen-image-2.1","nativeImageReference":{"method":"qwen-image-2.1"}})).1,32);
    }

    #[test]
    fn auto_refine_settings_reject_unknown_fields_and_bad_values() {
        assert!(Jobs::default().refine_slots_ok(20).is_ok());
        assert!(Jobs::default().refine_slots_ok(21).is_err());
        let good = json!({"checkpoint":"sam.safetensors","targets":["eyes","feet","hands"],"denoise":0.25,"margin":32,"threshold":0.5});
        let settings: AutoRefineSettings = serde_json::from_value(good.clone()).unwrap();
        settings.validate(&[]).unwrap();
        assert_eq!(settings.targets, ["eyes", "feet", "hands"]);
        let mut bad = good.clone();
        bad["steps"] = json!(4);
        assert!(serde_json::from_value::<AutoRefineSettings>(bad).is_err());
        let mut bad = settings.clone();
        bad.denoise = f64::NAN;
        assert!(bad.validate(&[]).is_err());
        assert!(
            serde_json::from_value::<RefineMask>(json!({"name":"a","png":[],"bounds":{}})).is_err()
        );
    }

    #[test]
    fn auto_refine_source_selects_latest_processed_output_not_original() {
        assert_eq!(
            output_index(
                &json!({"outputs":[{"role":"original"},{"role":"processed"},{"role":"original"}]})
            )
            .unwrap(),
            1
        );
        assert!(output_index(&json!({"outputs":[]})).is_err());
    }

    #[test]
    fn auto_refine_png_roundtrip_and_bad_boundary() {
        let p = Pixels {
            width: 2,
            height: 1,
            rgb: vec![0, 0, 0, 255, 255, 255],
        };
        let bytes = encode_png(&p).unwrap();
        assert_eq!(decode_png(&bytes).unwrap().rgb, p.rgb);
        assert!(decode_png(&bytes[..bytes.len() - 4]).is_err());
        assert!(decode_png(b"not png").is_err());
    }

    #[test]
    fn auto_refine_passes_chain_distinct_masks_and_preserve_generation_settings() {
        let mut base = json!({"recipeId":"checkpoint-text-to-image-diagnostic","version":1,
            "positivePrompt":"same prompt","negativePrompt":"","checkpoint":"model.safetensors",
            "seed":123,"width":512,"height":768,"steps":20,"cfg":7,
            "sampler":"euler","scheduler":"normal","outputPrefix":"DEAS_test",
            "loras":[{"name":"detail.safetensors","strength":0.7}],
            "inputLayers":[],"refine":{},"upscaleModel":"old.safetensors"});
        let mask1 = "deas-masks/00000000-0000-0000-0000-000000000001.png";
        let mask2 = "deas-masks/00000000-0000-0000-0000-000000000002.png";
        let crop1 =
            json!({"x":4,"y":8,"width":100,"height":80,"targetWidth":512,"targetHeight":512});
        let crop2 =
            json!({"x":200,"y":500,"width":80,"height":100,"targetWidth":512,"targetHeight":512});
        base["imageFormat"] = json!("webp");
        let first =
            pass_intent(&base, mask1, mask1, (512, 768), crop1.clone(), 0.25, true).unwrap();
        assert_eq!(first["imageFormat"], "png");
        let second =
            pass_intent(&base, mask2, mask2, (512, 768), crop2.clone(), 0.25, false).unwrap();
        assert_eq!(second["imageFormat"], "webp");
        assert_eq!(first["inpaintCrop"], crop1);
        assert_eq!(second["inpaintCrop"], crop2);
        assert_ne!(first["inputImage"], second["inputImage"]);
        for key in [
            "positivePrompt",
            "negativePrompt",
            "checkpoint",
            "seed",
            "steps",
            "cfg",
            "sampler",
            "scheduler",
            "loras",
        ] {
            assert_eq!(first[key], base[key]);
            assert_eq!(second[key], base[key]);
        }
        assert!(first.get("refine").is_none());
        assert!(first.get("inputLayers").is_none());
        base["sigmas"] = json!([10.0, 8.0, 5.0, 2.0, 0.0]);
        base["steps"] = json!(4);
        let fixed = pass_intent(&base, mask1, mask1, (512, 768), crop1, 0.5, false).unwrap();
        assert_eq!(fixed["sigmas"], json!([5.0, 2.0, 0.0]));
        assert_eq!(fixed["steps"], 2);
        let graph = compile(&first).unwrap();
        assert_eq!(graph["Y"]["class_type"], "ImageCompositeMasked");
        assert_eq!(graph["5"]["inputs"]["steps"], 20);
        assert_eq!(graph["5"]["inputs"]["denoise"], 0.25);
    }

    #[test]
    fn auto_refine_native_input_ids_and_masks_only_settings() {
        assert!(native_png_input(
            "deas-inputs/00000000-0000-0000-0000-000000000001.png"
        ));
        assert!(!native_png_input("deas-inputs/../secret.png"));
        assert!(!native_png_input("/etc/passwd"));
        let settings = AutoRefineSettings {
            checkpoint: String::new(),
            targets: vec![],
            denoise: 0.25,
            margin: 32,
            threshold: 0.5,
        };
        settings
            .validate(&[RefineMask {
                name: "paint".into(),
                png: vec![],
            }])
            .unwrap();
    }

    #[test]
    fn auto_refine_stop_fence_survives_stop_settling_between_jobs() {
        let jobs = Jobs::default();
        jobs.refine_fence(0).unwrap();
        jobs.lock().unwrap().admission_epoch = 1;
        assert!(jobs.refine_fence(0).is_err());
        jobs.refine_fence(1).unwrap();
        let (_tx, rx) = tokio::sync::watch::channel(None);
        jobs.lock().unwrap().stop = Some(rx);
        assert!(jobs.refine_fence(1).is_err());
    }
}
