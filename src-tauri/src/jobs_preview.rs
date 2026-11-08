//! Ephemeral display assets, never output/history/export authority.
#[cfg(test)]
#[path = "jobs_preview_tests.rs"]
mod tests;
use base64::Engine;
use serde_json::{json, Value};
use std::{
    io::{Cursor, Write},
    sync::Arc,
};

pub(crate) struct Preview {
    pub revision: u64,
    pub data: Value,
    pub video: Option<Arc<tempfile::NamedTempFile>>,
}

pub(super) fn bind_nodes(graph: &mut Value, id: &str) {
    let Some(nodes) = graph.as_object_mut() else {
        return;
    };
    let remap: Vec<_> = nodes
        .iter()
        .filter(|(_, n)| n["class_type"] == "ModelPreviewOverrideKJ")
        .map(|(key, _)| (key.clone(), format!("deas-preview-{id}-{key}")))
        .collect();
    for (old, new) in &remap {
        if let Some(node) = nodes.remove(old) {
            nodes.insert(new.clone(), node);
        }
    }
    for node in nodes.values_mut() {
        if let Some(inputs) = node["inputs"].as_object_mut() {
            for input in inputs.values_mut() {
                if let Some(link) = input
                    .as_array_mut()
                    .filter(|a| a.len() == 2 && a[1].is_u64())
                {
                    if let Some((_, new)) =
                        remap.iter().find(|(old, _)| link[0].as_str() == Some(old))
                    {
                        link[0] = json!(new);
                    }
                }
            }
        }
    }
}

pub(super) fn nodes(graph: &Value) -> Vec<String> {
    graph
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, n)| n["class_type"] == "ModelPreviewOverrideKJ")
        .map(|(id, _)| id.clone())
        .collect()
}

fn u24(b: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes([*b.first()?, *b.get(1)?, *b.get(2)?, 0]))
}
fn dimensions(w: u32, h: u32, budget: usize) -> Option<usize> {
    if w == 0 || h == 0 || w > 2048 || h > 2048 {
        return None;
    }
    (w as usize)
        .checked_mul(h as usize)?
        .checked_mul(4)
        .filter(|n| *n <= budget)
}

// Check inner coded sizes before image-webp allocates lossy frames, not only VP8X canvas sizes.
fn webp_chunks(bytes: &[u8], budget: usize, depth: usize) -> Option<()> {
    if depth > 1 {
        return None;
    }
    let mut offset = 0;
    while offset < bytes.len() {
        let head = bytes.get(offset..offset.checked_add(8)?)?;
        let len = u32::from_le_bytes(head[4..8].try_into().ok()?) as usize;
        let end = offset.checked_add(8)?.checked_add(len)?;
        let b = bytes.get(offset + 8..end)?;
        match &head[..4] {
            b"VP8 " => {
                if b.get(3..6)? != [0x9d, 1, 0x2a] {
                    return None;
                }
                dimensions(
                    u16::from_le_bytes(b.get(6..8)?.try_into().ok()?) as u32 & 0x3fff,
                    u16::from_le_bytes(b.get(8..10)?.try_into().ok()?) as u32 & 0x3fff,
                    budget,
                )?;
            }
            b"VP8L" => {
                if *b.first()? != 0x2f {
                    return None;
                }
                let bits = u32::from_le_bytes(b.get(1..5)?.try_into().ok()?);
                dimensions((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1, budget)?;
            }
            b"VP8X" => {
                dimensions(u24(b.get(4..7)?)? + 1, u24(b.get(7..10)?)? + 1, budget)?;
            }
            b"ANMF" => {
                dimensions(u24(b.get(6..9)?)? + 1, u24(b.get(9..12)?)? + 1, budget)?;
                webp_chunks(b.get(16..)?, budget, depth + 1)?;
            }
            _ => {}
        }
        offset = end.checked_add(len % 2)?;
    }
    (offset == bytes.len()).then_some(())
}

// H.264 may override the sample-entry dimensions in SPS NALs; bound those before playback too.
fn h264_sps(nal: &[u8], budget: usize) -> Option<()> {
    let mut rbsp = Vec::with_capacity(nal.len());
    let mut zeroes = 0;
    for &b in nal.get(1..)? {
        if zeroes >= 2 && b == 3 {
            zeroes = 0;
            continue;
        }
        rbsp.push(b);
        zeroes = if b == 0 { zeroes + 1 } else { 0 };
    }
    struct Bits<'a> {
        bytes: &'a [u8],
        pos: usize,
    }
    impl Bits<'_> {
        fn read(&mut self, n: usize) -> Option<u32> {
            if n > 32 {
                return None;
            }
            let mut v = 0;
            for _ in 0..n {
                v = (v << 1) | ((self.bytes.get(self.pos / 8)? >> (7 - self.pos % 8)) & 1) as u32;
                self.pos += 1;
            }
            Some(v)
        }
        fn ue(&mut self) -> Option<u32> {
            let mut n = 0;
            while self.read(1)? == 0 {
                n += 1;
                if n > 24 {
                    return None;
                }
            }
            Some((1u32 << n) - 1 + self.read(n)?)
        }
    }
    let mut bits = Bits {
        bytes: &rbsp,
        pos: 0,
    };
    let profile = bits.read(8)?;
    bits.read(16)?;
    bits.ue()?;
    if [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135].contains(&profile) {
        // Producer emits ordinary 8-bit 4:2:0, without custom scaling matrices.
        if bits.ue()? != 1 || bits.ue()? != 0 || bits.ue()? != 0 {
            return None;
        }
        bits.read(1)?;
        if bits.read(1)? != 0 {
            return None;
        }
    } else if ![66, 77, 88].contains(&profile) {
        return None;
    }
    if bits.ue()? > 12 {
        return None;
    }
    match bits.ue()? {
        0 => {
            if bits.ue()? > 12 {
                return None;
            }
        }
        1 => {
            bits.read(1)?;
            bits.ue()?;
            bits.ue()?;
            let count = bits.ue()?;
            if count > 256 {
                return None;
            }
            for _ in 0..count {
                bits.ue()?;
            }
        }
        2 => {}
        _ => return None,
    }
    let references = bits.ue()?;
    if references > 16 {
        return None;
    }
    bits.read(1)?;
    let w = bits.ue()?.checked_add(1)?.checked_mul(16)?;
    let h = bits.ue()?.checked_add(1)?.checked_mul(16)?;
    let progressive = bits.read(1)?;
    dimensions(
        w,
        h.checked_mul(2 - progressive)?,
        budget / (references as usize + 16),
    )?;
    Some(())
}
fn h264_nals(mut data: &[u8], budget: usize) -> Option<()> {
    while !data.is_empty() {
        let len = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
        let nal = data.get(4..4usize.checked_add(len)?)?;
        let kind = nal.first()? & 31;
        if kind == 7 {
            h264_sps(nal, budget)?;
        } else if ![1, 5, 6, 8, 9, 12].contains(&kind) {
            return None;
        }
        data = data.get(4 + len..)?;
    }
    Some(())
}
fn avc_config(data: &[u8], budget: usize) -> Option<()> {
    let mut pos = 78usize;
    let mut found = false;
    while pos < data.len() {
        let len = u32::from_be_bytes(data.get(pos..pos + 4)?.try_into().ok()?) as usize;
        if len < 8 {
            return None;
        }
        let tag = data.get(pos + 4..pos + 8)?;
        let b = data.get(pos + 8..pos.checked_add(len)?)?;
        if tag == b"avcC" {
            if *b.first()? != 1 || b.get(4)? & 3 != 3 || b.get(5)? & 31 != 1 {
                return None;
            }
            let size = u16::from_be_bytes(b.get(6..8)?.try_into().ok()?) as usize;
            h264_sps(b.get(8..8 + size)?, budget)?;
            found = true;
        }
        pos += len;
    }
    found.then_some(())
}
// shortcut: accept self-contained H.264 MP4 (plain or PyAV frag_keyframe+empty_moov fragmented); extend when the producer changes codecs.
fn mp4(bytes: &[u8], w: u32, h: u32, budget: usize) -> Option<()> {
    fn boxes(
        b: &[u8],
        depth: usize,
        w: u32,
        h: u32,
        budget: usize,
        seen: &mut [bool; 3],
    ) -> Option<()> {
        if depth > 8 {
            return None;
        }
        let mut pos = 0;
        while pos < b.len() {
            let head = b.get(pos..pos.checked_add(8)?)?;
            let len = u32::from_be_bytes(head[..4].try_into().ok()?) as usize;
            if len < 8 {
                return None;
            }
            let end = pos.checked_add(len)?;
            let data = b.get(pos + 8..end)?;
            match &head[4..8] {
                b"ftyp" => {
                    if data.len() < 8 {
                        return None;
                    }
                    seen[0] = true;
                }
                b"mdat" => {
                    if data.is_empty() {
                        return None;
                    }
                    h264_nals(data, budget)?;
                    seen[1] = true;
                }
                // Fragmented streams keep samples in moof/traf; their bytes are still bounded by the mdat walk below.
                b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"dinf" | b"mvex" | b"moof"
                | b"traf" => boxes(data, depth + 1, w, h, budget, seen)?,
                b"stsd" => {
                    if u32::from_be_bytes(data.get(4..8)?.try_into().ok()?) != 1 {
                        return None;
                    }
                    if data.get(12..16)? != b"avc1" {
                        return None;
                    }
                    boxes(data.get(8..)?, depth + 1, w, h, budget, seen)?;
                }
                b"avc1" => {
                    let cw = u16::from_be_bytes(data.get(24..26)?.try_into().ok()?) as u32;
                    let ch = u16::from_be_bytes(data.get(26..28)?.try_into().ok()?) as u32;
                    dimensions(cw, ch, budget)?;
                    if cw != w || ch != h {
                        return None;
                    }
                    avc_config(data, budget)?;
                    seen[2] = true;
                }
                b"dref" => {
                    if u32::from_be_bytes(data.get(4..8)?.try_into().ok()?) != 1
                        || data.get(12..16)? != b"url "
                        || data.get(16..20)? != [0, 0, 0, 1]
                    {
                        return None;
                    }
                }
                b"mdhd" => {
                    let (scale, duration) = match data.first()? {
                        0 => (
                            u32::from_be_bytes(data.get(12..16)?.try_into().ok()?) as u64,
                            u32::from_be_bytes(data.get(16..20)?.try_into().ok()?) as u64,
                        ),
                        1 => (
                            u32::from_be_bytes(data.get(20..24)?.try_into().ok()?) as u64,
                            u64::from_be_bytes(data.get(24..32)?.try_into().ok()?),
                        ),
                        _ => return None,
                    };
                    // Fragmented streams report duration 0 in the empty moov; the byte budget bounds them instead.
                    if scale == 0 || duration > scale * 30 {
                        return None;
                    }
                }
                b"stsz" => {
                    // empty_moov fragmentation reports sample count 0 here.
                    let count = u32::from_be_bytes(data.get(8..12)?.try_into().ok()?) as usize;
                    if count > 256 {
                        return None;
                    }
                }
                b"stts" => {
                    let count = u32::from_be_bytes(data.get(4..8)?.try_into().ok()?) as usize;
                    if count > 256 || data.len() != 8 + count * 8 {
                        return None;
                    }
                    let mut frames = 0u64;
                    for row in data[8..].chunks_exact(8) {
                        frames += u32::from_be_bytes(row[..4].try_into().ok()?) as u64;
                        if frames > 256 || row[4..] == [0, 0, 0, 0] {
                            return None;
                        }
                    }
                }
                b"encv" | b"hvc1" | b"hev1" => return None,
                _ => {}
            }
            pos = end;
        }
        Some(())
    }
    let mut seen = [false; 3];
    boxes(bytes, 0, w, h, budget, &mut seen)?;
    seen.iter().all(|v| *v).then_some(())
}

pub(super) fn parse(data: &Value, job: &str, revision: u64, budget: usize) -> Option<Preview> {
    let node = data["node_id"].as_str()?;
    let encoded = data["image"].as_str()?;
    if encoded.len() > (budget / 8).min(super::RESPONSE_LIMIT * 16) || encoded.is_empty() {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let w = u32::try_from(data["w"].as_u64()?).ok()?;
    let h = u32::try_from(data["h"].as_u64()?).ok()?;
    dimensions(w, h, budget)?;
    let mime = data["mime"].as_str().unwrap_or("image/jpeg");
    let fps = data["fps"].as_f64().unwrap_or(1.0);
    if !fps.is_finite() || !(1.0..=60.0).contains(&fps) {
        return None;
    }
    let mut value = json!({"revision":revision,"nodeId":node,"width":w,"height":h,"fps":fps});
    let mut video = None;
    match mime {
        "video/mp4" => {
            mp4(&bytes, w, h, budget)?;
            let mut file = tempfile::Builder::new()
                .prefix("deas-preview-")
                .suffix(".mp4")
                .tempfile()
                .ok()?;
            file.write_all(&bytes).ok()?;
            value["video"] = json!({"previewJobId":job,"previewRevision":revision});
            video = Some(Arc::new(file));
        }
        "image/webp" => {
            if bytes.get(..4)? != b"RIFF"
                || bytes.get(8..12)? != b"WEBP"
                || u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize + 8 != bytes.len()
            {
                return None;
            }
            webp_chunks(bytes.get(12..)?, budget, 0)?;
            let mut decoder = image_webp::WebPDecoder::new(Cursor::new(&bytes)).ok()?;
            decoder.set_memory_limit(budget);
            if decoder.dimensions() != (w, h)
                || decoder.num_frames() == 0
                || decoder.num_frames() > 256
            {
                return None;
            }
            let size = decoder.output_buffer_size()?;
            // Pixels + PNG + base64 + serialized IPC can coexist; reserve all copies up front.
            if size
                .checked_mul(decoder.num_frames() as usize)?
                .checked_mul(8)?
                > budget
            {
                return None;
            }
            let alpha = decoder.has_alpha();
            let mut pixels = vec![0; size];
            let mut frames = Vec::new();
            let mut total = 0u32;
            for _ in 0..decoder.num_frames() {
                let duration = decoder.read_frame(&mut pixels).ok()?;
                if duration == 0 || duration > 1000 {
                    return None;
                }
                total = total.checked_add(duration)?;
                if total > 30_000 {
                    return None;
                }
                let mut png = Vec::new();
                {
                    let mut encoder = png::Encoder::new(&mut png, w, h);
                    encoder.set_color(if alpha {
                        png::ColorType::Rgba
                    } else {
                        png::ColorType::Rgb
                    });
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .ok()?
                        .write_image_data(&pixels)
                        .ok()?;
                }
                frames.push(json!({"src":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(png)),"durationMs":duration}));
            }
            value["frames"] = json!(frames);
        }
        _ => return None,
    }
    Some(Preview {
        revision,
        data: value,
        video,
    })
}

pub(super) fn jpeg(data: &Value, budget: usize) -> Option<String> {
    if data.get("mime").is_some_and(|m| m != "image/jpeg") {
        return None;
    }
    let encoded = data["image"].as_str()?;
    if encoded.len() > (budget / 8).min(super::RESPONSE_LIMIT * 16) {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return None;
    }
    let mut pos = 2usize;
    while pos < bytes.len() {
        if *bytes.get(pos)? != 0xff {
            return None;
        }
        let marker = *bytes.get(pos + 1)?;
        if marker == 0xff {
            pos += 1;
            continue;
        }
        if marker == 0xda || marker == 0xd9 {
            return None;
        }
        let len = u16::from_be_bytes(bytes.get(pos + 2..pos + 4)?.try_into().ok()?) as usize;
        if len < 2 {
            return None;
        }
        let segment = bytes.get(pos + 4..pos.checked_add(2 + len)?)?;
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let h = u16::from_be_bytes(segment.get(1..3)?.try_into().ok()?) as u32;
            let w = u16::from_be_bytes(segment.get(3..5)?.try_into().ok()?) as u32;
            dimensions(w, h, budget)?;
            return Some(format!("data:image/jpeg;base64,{encoded}"));
        }
        pos = pos.checked_add(2 + len)?;
    }
    None
}
