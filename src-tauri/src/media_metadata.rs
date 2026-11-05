//! Read-only embedded text extraction; this is not image decoding or image integrity validation.
use flate2::{Decompress, FlushDecompress, Status};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_TEXT: usize = 1024 * 1024;
const MAX_TOTAL: usize = 2 * MAX_TEXT;
const MAX_ENTRIES: usize = 64;
const MAX_CHUNKS: usize = 100_000;
const MAX_PAYLOAD: usize = MAX_TEXT + 1024;
type Entries = Vec<(String, String)>;

#[tauri::command]
pub async fn native_media_metadata(path: String) -> Result<Vec<(String, String)>, String> {
    let result = tauri::async_runtime::spawn_blocking(move || read_metadata(Path::new(&path)))
        .await
        .map_err(|_| "Media metadata reader failed".to_string())
        .and_then(|value| value);
    if let Err(error) = &result {
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!("ERROR [media.metadata] read: {error}"),
        );
    }
    result
}

pub fn read_metadata(path: &Path) -> Result<Vec<(String, String)>, String> {
    if !path.is_absolute() {
        return Err("Media path must be absolute".into());
    }
    // Check before opening too: opening a named pipe could otherwise block indefinitely.
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("Media path must be a regular file".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Avoid blocking if a regular file is replaced with a FIFO between checks.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Err("Media must be a regular file no larger than 512 MiB".into());
    }
    let size = meta.len();
    let mut signature = [0; 12];
    let count = file.read(&mut signature).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if count >= 8 && signature[..8] == *b"\x89PNG\r\n\x1a\n" {
        file.seek(SeekFrom::Start(8)).map_err(|e| e.to_string())?;
        png(&mut file, size, &mut out)?;
    } else if count >= 2 && signature[..2] == *b"\xff\xd8" {
        file.seek(SeekFrom::Start(2)).map_err(|e| e.to_string())?;
        jpeg(&mut file, size, &mut out)?;
    } else if count >= 12 && signature[..4] == *b"RIFF" && signature[8..12] == *b"WEBP" {
        let end = u32::from_le_bytes(signature[4..8].try_into().unwrap()) as u64 + 8;
        if end != size {
            return Err("Invalid WebP RIFF length".into());
        }
        file.seek(SeekFrom::Start(12)).map_err(|e| e.to_string())?;
        webp(&mut file, end, &mut out)?;
    } else if count >= 8 && &signature[4..8] == b"ftyp" {
        mp4(&mut file, size, &mut out)?;
    } else if count >= 4 && signature[..4] == [0x1a, 0x45, 0xdf, 0xa3] {
        matroska(&mut file, size, &mut out)?;
    }
    Ok(out)
}

/// Video tags ComfyUI (SaveVideo/SaveWEBM, VHS, DaSiWa Enhanced Video Combine) write and Civitai reads.
const VIDEO_TAGS: [&str; 2] = ["prompt", "workflow"];
const MAX_META: u64 = 8 * 1024 * 1024;

fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        b.get(at..at + 4)
            .ok_or("Truncated box")?
            .try_into()
            .unwrap(),
    ))
}

/// (type, body start, end) of the box at `at`, bounded by `end`.
fn mp4_box(file: &mut File, at: u64, end: u64) -> Result<Option<([u8; 4], u64, u64)>, String> {
    if at + 8 > end {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
    let header = read::<8>(file)?;
    let (mut size, mut body) = (
        u32::from_be_bytes(header[..4].try_into().unwrap()) as u64,
        at + 8,
    );
    if size == 1 {
        size = u64::from_be_bytes(read::<8>(file)?);
        body = at + 16;
    } else if size == 0 {
        size = end - at;
    }
    let stop = at.checked_add(size).ok_or("Invalid box size")?;
    if stop < body || stop > end {
        return Err("Invalid MP4 box".into());
    }
    Ok(Some((header[4..].try_into().unwrap(), body, stop)))
}

fn mp4_find(
    file: &mut File,
    mut at: u64,
    end: u64,
    kind: &[u8; 4],
) -> Result<Option<(u64, u64)>, String> {
    for _ in 0..MAX_CHUNKS {
        match mp4_box(file, at, end)? {
            Some((k, body, stop)) if &k == kind => return Ok(Some((body, stop))),
            Some((_, _, stop)) => at = stop,
            None => return Ok(None),
        }
    }
    Err("Too many MP4 boxes".into())
}

/// QuickTime `mdta` keys (`-movflags use_metadata_tags`): `moov/[udta/]meta/{keys,ilst}`.
fn mp4(file: &mut File, size: u64, out: &mut Entries) -> Result<(), String> {
    let Some((moov, moov_end)) = mp4_find(file, 0, size, b"moov")? else {
        return Ok(());
    };
    // ffmpeg nests `meta` in `udta`; Apple writers put it directly under `moov`.
    let meta = match mp4_find(file, moov, moov_end, b"meta")? {
        Some(m) => Some(m),
        None => match mp4_find(file, moov, moov_end, b"udta")? {
            Some((udta, udta_end)) => mp4_find(file, udta, udta_end, b"meta")?,
            None => None,
        },
    };
    let Some((meta, meta_end)) = meta else {
        return Ok(());
    };
    if meta_end - meta > MAX_META {
        return Err("MP4 metadata exceeds size limit".into());
    }
    file.seek(SeekFrom::Start(meta))
        .map_err(|e| e.to_string())?;
    let mut b = vec![0; (meta_end - meta) as usize];
    file.read_exact(&mut b).map_err(|e| e.to_string())?;
    // `meta` is a full box (version/flags) in ISO BMFF but a plain box in QuickTime.
    let start = if b.get(4..8) == Some(b"hdlr") { 0 } else { 4 };
    let children =
        |b: &[u8], mut at: usize, end: usize| -> Result<Vec<([u8; 4], usize, usize)>, String> {
            let mut list = Vec::new();
            while at + 8 <= end && list.len() < MAX_CHUNKS {
                let size = be32(b, at)? as usize;
                if size < 8 || at + size > end {
                    break;
                }
                list.push((b[at + 4..at + 8].try_into().unwrap(), at + 8, at + size));
                at += size;
            }
            Ok(list)
        };
    let top = children(&b, start, b.len())?;
    let (Some(keys), Some(ilst)) = (
        top.iter().find(|c| &c.0 == b"keys"),
        top.iter().find(|c| &c.0 == b"ilst"),
    ) else {
        return Ok(());
    };
    let mut names = Vec::new();
    let count = be32(&b, keys.1 + 4)? as usize;
    let mut at = keys.1 + 8;
    while names.len() < count.min(MAX_CHUNKS) && at + 8 <= keys.2 {
        let size = be32(&b, at)? as usize;
        if size < 8 || at + size > keys.2 {
            break;
        }
        names.push(b[at + 8..at + size].to_vec());
        at += size;
    }
    for (_, body, end) in children(&b, ilst.1, ilst.2)? {
        let index = be32(&b, body - 4)? as usize;
        let Some(name) = index.checked_sub(1).and_then(|i| names.get(i)) else {
            continue;
        };
        let name = String::from_utf8_lossy(name).to_lowercase();
        if !VIDEO_TAGS.contains(&name.as_str()) || out.iter().any(|(k, _)| *k == name) {
            continue;
        }
        // data: type indicator (4) + locale (4); type 1 is UTF-8 text
        if let Some(data) = children(&b, body, end)?.iter().find(|c| &c.0 == b"data") {
            if data.2 - data.1 >= 8 && be32(&b, data.1)? == 1 {
                push(out, name, utf8(&b[data.1 + 8..data.2])?)?;
            }
        }
    }
    Ok(())
}

/// EBML variable-size integer at `at`; `None` for the all-ones "unknown size".
fn vint(b: &[u8], at: usize, strip: bool) -> Result<(Option<u64>, usize), String> {
    let first = *b.get(at).ok_or("Truncated EBML")?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return Err("Invalid EBML integer".into());
    }
    let bytes = b.get(at..at + len).ok_or("Truncated EBML")?;
    let marker = 0x80u8 >> (len - 1);
    let mut value = u64::from(if strip { first & (marker - 1) } else { first });
    let mut ones = first & (marker - 1) == marker - 1;
    for &x in &bytes[1..] {
        value = (value << 8) | u64::from(x);
        ones &= x == 0xff;
    }
    Ok((if strip && ones { None } else { Some(value) }, len))
}

/// (id, body start, size) of the element at `at`.
fn ebml(b: &[u8], at: usize) -> Result<(u64, usize, Option<u64>), String> {
    let (id, id_len) = vint(b, at, false)?;
    let (size, size_len) = vint(b, at + id_len, true)?;
    Ok((id.unwrap(), at + id_len + size_len, size))
}

fn simple_tags(
    b: &[u8],
    mut at: usize,
    end: usize,
    depth: usize,
    out: &mut Entries,
) -> Result<(), String> {
    let (mut name, mut value) = (None, None);
    while at < end && depth <= 6 {
        let (id, body, size) = ebml(b, at)?;
        let stop = body + size.ok_or("Unknown-size tag element")? as usize;
        if stop > end {
            return Err("Truncated Matroska tag".into());
        }
        match id {
            0x7373 | 0x67c8 => simple_tags(b, body, stop, depth + 1, out)?,
            0x45a3 => name = Some(String::from_utf8_lossy(&b[body..stop]).to_lowercase()),
            0x4487 => value = Some(utf8(&b[body..stop])?),
            _ => {}
        }
        at = stop;
    }
    if let (Some(name), Some(value)) = (name, value) {
        if VIDEO_TAGS.contains(&name.as_str()) && !out.iter().any(|(k, _)| *k == name) {
            push(out, name, value)?;
        }
    }
    Ok(())
}

/// Matroska/WebM global `Tags` (ffmpeg writes them before the clusters).
fn matroska(file: &mut File, size: u64, out: &mut Entries) -> Result<(), String> {
    let mut at = 0u64;
    let mut segment_end = size;
    for _ in 0..MAX_CHUNKS {
        if at + 2 > segment_end {
            return Ok(());
        }
        file.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
        let mut head = [0; 16];
        let n = file.read(&mut head).map_err(|e| e.to_string())?;
        let (id, body, len) = ebml(&head[..n], 0)?;
        let body = at + body as u64;
        match (id, len) {
            // A Segment may be unknown-size (streamed) or cut short; it runs to the end of the file.
            (0x18538067, len) => {
                segment_end = len.map_or(size, |l| (body + l).min(size));
                at = body;
            }
            (0x1254c367, Some(len)) => {
                if len > MAX_META {
                    return Err("Matroska tags exceed size limit".into());
                }
                file.seek(SeekFrom::Start(body))
                    .map_err(|e| e.to_string())?;
                let mut b = vec![0; len as usize];
                file.read_exact(&mut b).map_err(|e| e.to_string())?;
                simple_tags(&b, 0, b.len(), 0, out)?;
                at = body + len;
            }
            (_, Some(len)) => at = body + len,
            // An unknown-size Cluster cannot be skipped; tags precede clusters, so stop here.
            (_, None) => return Ok(()),
        }
    }
    Err("Too many Matroska elements".into())
}

fn read<const N: usize>(file: &mut File) -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn position(file: &mut File) -> Result<u64, String> {
    file.stream_position().map_err(|e| e.to_string())
}

fn end_of(file: &mut File, length: u64, limit: u64) -> Result<u64, String> {
    let end = position(file)?
        .checked_add(length)
        .ok_or("Invalid chunk length")?;
    if end > limit {
        return Err("Truncated metadata chunk".into());
    }
    Ok(end)
}

fn payload(file: &mut File, length: usize) -> Result<Vec<u8>, String> {
    if length > MAX_PAYLOAD {
        return Err("Metadata chunk exceeds size limit".into());
    }
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn push(out: &mut Entries, key: String, value: String) -> Result<(), String> {
    let size = key.len() + value.len();
    if value.len() > MAX_TEXT
        || out.len() >= MAX_ENTRIES
        || out.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() + size > MAX_TOTAL
    {
        return Err("Embedded metadata exceeds limits".into());
    }
    out.push((key, value));
    Ok(())
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| char::from(byte)).collect()
}

fn utf8(bytes: &[u8]) -> Result<String, String> {
    String::from_utf8(bytes.to_vec()).map_err(|_| "Invalid UTF-8 metadata".into())
}

fn inflate(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut result = Vec::with_capacity(MAX_TEXT + 1);
    let mut decoder = Decompress::new(true);
    let status = decoder
        .decompress_vec(bytes, &mut result, FlushDecompress::Finish)
        .map_err(|e| e.to_string())?;
    if result.len() > MAX_TEXT {
        return Err("Decompressed metadata exceeds 1 MiB".into());
    }
    if status != Status::StreamEnd || decoder.total_in() != bytes.len() as u64 {
        return Err("Incomplete or trailing compressed metadata".into());
    }
    Ok(result)
}

fn nul(bytes: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let split = bytes
        .iter()
        .position(|&b| b == 0)
        .ok_or("Missing metadata separator")?;
    Ok((&bytes[..split], &bytes[split + 1..]))
}

fn png(file: &mut File, size: u64, out: &mut Entries) -> Result<(), String> {
    for index in 0..MAX_CHUNKS {
        end_of(file, 8, size)?;
        let header = read::<8>(file)?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let kind = &header[4..];
        let end = end_of(file, length as u64 + 4, size)?;
        if index == 0 && (kind != b"IHDR" || length != 13) {
            return Err("Missing PNG IHDR".into());
        }
        if matches!(kind, b"tEXt" | b"zTXt" | b"iTXt") {
            let data = payload(file, length)?;
            let (key, rest) = nul(&data)?;
            if key.is_empty() || key.len() > 79 {
                return Err("Invalid PNG text keyword".into());
            }
            let value = match kind {
                b"tEXt" => latin1(rest),
                b"zTXt" => {
                    if rest.first() != Some(&0) {
                        return Err("Unsupported PNG text compression".into());
                    }
                    latin1(&inflate(&rest[1..])?)
                }
                _ => {
                    if rest.len() < 2 || rest[0] > 1 || rest[1] != 0 {
                        return Err("Invalid PNG international text compression".into());
                    }
                    let (_, rest) = nul(&rest[2..])?; // language tag
                    let (_, text) = nul(rest)?; // translated keyword; preserve the original key
                    if data[key.len() + 1] == 1 {
                        utf8(&inflate(text)?)?
                    } else {
                        utf8(text)?
                    }
                }
            };
            push(out, latin1(key), value)?;
        }
        // IDAT and all other non-text chunks are sought over, never decoded or allocated.
        file.seek(SeekFrom::Start(end)).map_err(|e| e.to_string())?;
        if kind == b"IEND" {
            return if length == 0 && end == size {
                Ok(())
            } else {
                Err("Invalid PNG IEND".into())
            };
        }
    }
    Err("Too many PNG chunks".into())
}

fn jpeg(file: &mut File, size: u64, out: &mut Entries) -> Result<(), String> {
    for _ in 0..MAX_CHUNKS {
        if read::<1>(file)? != [0xff] {
            return Err("Invalid JPEG marker".into());
        }
        let marker = read::<1>(file)?[0];
        // Count fill bytes as traversal too, rather than allowing an unbounded inner loop.
        if marker == 0xff {
            file.seek(SeekFrom::Current(-1))
                .map_err(|e| e.to_string())?;
            continue;
        }
        if marker == 0xda || marker == 0xd9 {
            return Ok(()); // No entropy-stream parsing: metadata after SOS is not inspected.
        }
        if marker == 0 || marker == 0xd8 {
            return Err("Invalid JPEG marker".into());
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = u16::from_be_bytes(read::<2>(file)?) as usize;
        if length < 2 {
            return Err("Invalid JPEG segment length".into());
        }
        let end = end_of(file, (length - 2) as u64, size)?;
        if marker == 0xfe || marker == 0xe1 {
            let data = payload(file, length - 2)?;
            if marker == 0xfe {
                let text = std::str::from_utf8(&data)
                    .map(str::to_owned)
                    .unwrap_or_else(|_| latin1(&data));
                push(out, "Comment".into(), text)?;
            } else if let Some(tiff) = data.strip_prefix(b"Exif\0\0") {
                exif(tiff, out)?;
            } else if let Some(xmp) = data.strip_prefix(b"http://ns.adobe.com/xap/1.0/\0") {
                push(out, "XML:com.adobe.xmp".into(), utf8(xmp)?)?;
            }
        }
        file.seek(SeekFrom::Start(end)).map_err(|e| e.to_string())?;
    }
    Err("Too many JPEG markers".into())
}

fn webp(file: &mut File, size: u64, out: &mut Entries) -> Result<(), String> {
    for _ in 0..MAX_CHUNKS {
        if position(file)? == size {
            return Ok(());
        }
        end_of(file, 8, size)?;
        let header = read::<8>(file)?;
        let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        let end = end_of(file, length as u64 + (length as u64 & 1), size)?;
        match &header[..4] {
            b"EXIF" => {
                let data = payload(file, length)?;
                exif(data.strip_prefix(b"Exif\0\0").unwrap_or(&data), out)?;
            }
            b"XMP " => push(
                out,
                "XML:com.adobe.xmp".into(),
                utf8(&payload(file, length)?)?,
            )?,
            _ => {}
        }
        file.seek(SeekFrom::Start(end)).map_err(|e| e.to_string())?;
    }
    Err("Too many WebP chunks".into())
}

// shortcut: only IFD0 and its ExifIFD, add an audited TIFF dependency for broader EXIF support.
fn exif(data: &[u8], out: &mut Entries) -> Result<(), String> {
    let little = match data.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return Err("Invalid EXIF byte order".into()),
    };
    let number = |offset: usize, width: usize| -> Result<u32, String> {
        let bytes = data
            .get(offset..offset.checked_add(width).ok_or("EXIF offset overflow")?)
            .ok_or("Truncated EXIF field")?;
        Ok(if little {
            bytes.iter().rev().fold(0, |n, &b| (n << 8) | u32::from(b))
        } else {
            bytes.iter().fold(0, |n, &b| (n << 8) | u32::from(b))
        })
    };
    if number(2, 2)? != 42 {
        return Err("Unsupported EXIF TIFF header".into());
    }
    let mut offset = number(4, 4)? as usize;
    for directory in 0..2 {
        if offset == 0 {
            break;
        }
        let count = number(offset, 2)? as usize;
        if count > 4096 {
            return Err("Too many EXIF fields".into());
        }
        let table = offset.checked_add(2).ok_or("EXIF offset overflow")?;
        let end = table
            .checked_add(count * 12 + 4)
            .ok_or("EXIF offset overflow")?;
        if end > data.len() {
            return Err("Truncated EXIF directory".into());
        }
        let mut exif_offset = 0;
        for index in 0..count {
            let entry = table + index * 12;
            let tag = number(entry, 2)?;
            let kind = number(entry + 2, 2)?;
            let count = number(entry + 4, 4)? as usize;
            if tag == 0x8769 && directory == 0 && kind == 4 && count == 1 {
                exif_offset = number(entry + 8, 4)? as usize;
            }
            if !(((tag == 0x010e || tag == 0x0110) && kind == 2) || (tag == 0x9286 && kind == 7)) {
                continue;
            }
            if count > MAX_TEXT {
                return Err("EXIF text exceeds size limit".into());
            }
            let start = if count <= 4 {
                entry + 8
            } else {
                number(entry + 8, 4)? as usize
            };
            let end = start.checked_add(count).ok_or("EXIF offset overflow")?;
            let bytes = data.get(start..end).ok_or("Truncated EXIF text")?;
            let (key, value) = if tag == 0x010e || tag == 0x0110 {
                (
                    if tag == 0x0110 {
                        "Model"
                    } else {
                        "ImageDescription"
                    },
                    latin1(bytes.strip_suffix(&[0]).unwrap_or(bytes)),
                )
            } else if let Some(text) = bytes.strip_prefix(b"ASCII\0\0\0") {
                (
                    "UserComment",
                    latin1(text.strip_suffix(&[0]).unwrap_or(text)),
                )
            } else if let Some(text) = bytes.strip_prefix(b"UNICODE\0") {
                let (text, le) = if let Some(text) = text.strip_prefix(&[0xff, 0xfe]) {
                    (text, true)
                } else if let Some(text) = text.strip_prefix(&[0xfe, 0xff]) {
                    (text, false)
                } else {
                    (text, little)
                };
                if text.len() % 2 != 0 {
                    return Err("Invalid EXIF Unicode length".into());
                }
                let mut units: Vec<u16> = text
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| {
                        if le {
                            u16::from_le_bytes([b[0], b[1]])
                        } else {
                            u16::from_be_bytes([b[0], b[1]])
                        }
                    })
                    .collect();
                if units.last() == Some(&0) {
                    units.pop();
                }
                (
                    "UserComment",
                    String::from_utf16(&units).map_err(|_| "Invalid EXIF Unicode")?,
                )
            } else {
                // Undefined/JIS comments are binary, not losslessly representable text.
                continue;
            };
            push(out, key.into(), value)?;
        }
        offset = exif_offset;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn metadata_command_errors_reach_shared_logs_without_paths() {
        let after = crate::logs::native_logs(0)
            .last()
            .map_or(0, |entry| entry.seq);
        let error = native_media_metadata("private-prompt-name.png".into())
            .await
            .unwrap_err();
        assert_eq!(error, "Media path must be absolute");
        let entries = crate::logs::native_logs(after);
        assert!(entries.iter().any(|entry| {
            entry.source == crate::logs::Source::Deas
                && entry.text == "ERROR [media.metadata] read: Media path must be absolute"
        }));
        assert!(
            !entries
                .iter()
                .any(|entry| entry.text.contains("private-prompt-name"))
        );
    }
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new(data: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "deas-metadata-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            file.write_all(data).unwrap();
            Self(path)
        }
        fn read(&self) -> Result<Entries, String> {
            read_metadata(&self.0)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    fn encoded_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = ::png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(::png::ColorType::Grayscale);
            encoder
                .add_text_chunk("parameters".into(), "cat\nSteps: 20".into())
                .unwrap();
            encoder
                .add_ztxt_chunk("prompt".into(), "{\"seed\":42}".into())
                .unwrap();
            encoder
                .add_itxt_chunk("workflow".into(), "猫 — Grüße".into())
                .unwrap();
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[0])
                .unwrap();
        }
        bytes
    }
    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }
    fn insert_chunk(bytes: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        // Use png's encoder to generate valid CRCs for injected text chunks.
        let mut encoded = Vec::new();
        {
            let encoder = ::png::Encoder::new(&mut encoded, 1, 1);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_chunk(::png::chunk::ChunkType(*kind), data)
                .unwrap();
            writer.write_image_data(&[0]).unwrap();
        }
        let length = 12 + data.len();
        bytes.splice(33..33, encoded[33..33 + length].iter().copied());
    }
    #[test]
    fn real_encoded_text_chunks_and_unicode() {
        let mut bytes = encoded_png();
        let mut text = b"unicode\0\x01\0ja\0translated\0".to_vec();
        text.extend(zlib("日本語 🐈\n".as_bytes()));
        insert_chunk(&mut bytes, b"iTXt", &text);
        let entries = Fixture::new(&bytes).read().unwrap();
        assert!(entries.contains(&("parameters".into(), "cat\nSteps: 20".into())));
        assert!(entries.contains(&("prompt".into(), "{\"seed\":42}".into())));
        assert!(entries.contains(&("workflow".into(), "猫 — Grüße".into())));
        assert!(entries.contains(&("unicode".into(), "日本語 🐈\n".into())));
    }
    #[test]
    fn absent_and_unsupported() {
        let mut bytes = Vec::new();
        {
            ::png::Encoder::new(&mut bytes, 1, 1)
                .write_header()
                .unwrap()
                .write_image_data(&[0])
                .unwrap();
        }
        assert!(Fixture::new(&bytes).read().unwrap().is_empty());
        assert!(
            Fixture::new(b"unsupported media")
                .read()
                .unwrap()
                .is_empty()
        );
        assert!(read_metadata(Path::new("relative.png")).is_err());
        assert!(read_metadata(&std::env::temp_dir()).is_err());
    }
    #[test]
    fn truncated_and_malformed() {
        let bytes = encoded_png();
        for length in [8, 12, 20, bytes.len() - 1] {
            assert!(Fixture::new(&bytes[..length]).read().is_err());
        }
        let mut bytes = encoded_png();
        insert_chunk(&mut bytes, b"iTXt", b"bad\0\x02\0\0\0text");
        assert!(Fixture::new(&bytes).read().is_err());
        assert!(exif(b"II*\0\xff\xff\xff\xff", &mut Vec::new()).is_err());
        let mut bytes = encoded_png();
        insert_chunk(&mut bytes, b"zTXt", b"bad\0\0not zlib");
        assert!(Fixture::new(&bytes).read().is_err());
        let compressed = zlib(b"intact");
        assert!(inflate(&compressed[..compressed.len() - 1]).is_err());
        assert_eq!(
            inflate(&zlib(&vec![b'x'; MAX_TEXT])).unwrap().len(),
            MAX_TEXT
        );
    }
    #[test]
    fn decompression_bomb_and_entry_limits() {
        let mut bytes = encoded_png();
        let mut text = b"bomb\0\0".to_vec();
        text.extend(zlib(&vec![b'x'; MAX_TEXT + 1]));
        insert_chunk(&mut bytes, b"zTXt", &text);
        assert!(
            Fixture::new(&bytes)
                .read()
                .unwrap_err()
                .contains("Decompressed")
        );
        let mut bytes = encoded_png();
        for _ in 0..MAX_ENTRIES {
            insert_chunk(&mut bytes, b"tEXt", b"key\0value");
        }
        assert!(Fixture::new(&bytes).read().is_err());
        let mut out = Vec::new();
        push(&mut out, "a".into(), "x".repeat(MAX_TEXT)).unwrap();
        assert!(push(&mut out, "b".into(), "x".repeat(MAX_TEXT)).is_err());
    }
    #[test]
    fn jpeg_comment_and_webp_xmp() {
        let jpeg = b"\xff\xd8\xff\xfe\0\x07hello\xff\xd9";
        assert_eq!(
            Fixture::new(jpeg).read().unwrap(),
            vec![("Comment".into(), "hello".into())]
        );
        let mut webp = b"RIFF\x10\0\0\0WEBPXMP \x03\0\0\0abc\0".to_vec();
        assert_eq!(
            Fixture::new(&webp).read().unwrap(),
            vec![("XML:com.adobe.xmp".into(), "abc".into())]
        );
        webp.pop();
        assert!(Fixture::new(&webp).read().is_err());
    }
    #[test]
    fn swarm_model_in_jpeg_and_webp() {
        let text = br#"{"sui_image_params":{"seed":42},"sui_extra_data":{}}"#;
        // Big-endian IFD0 Model with an out-of-line ASCII value.
        let mut tiff = b"MM\0*\0\0\0\x08\0\x01\x01\x10\0\x02".to_vec();
        tiff.extend_from_slice(&((text.len() + 1) as u32).to_be_bytes());
        tiff.extend_from_slice(&26u32.to_be_bytes());
        tiff.extend_from_slice(&[0; 4]);
        tiff.extend_from_slice(text);
        tiff.push(0);
        let mut jpeg = b"\xff\xd8\xff\xe1".to_vec();
        jpeg.extend_from_slice(&((tiff.len() + 8) as u16).to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);
        jpeg.extend_from_slice(b"\xff\xd9");
        let expected = vec![("Model".into(), String::from_utf8(text.to_vec()).unwrap())];
        assert_eq!(Fixture::new(&jpeg).read().unwrap(), expected);
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&((12 + tiff.len() + (tiff.len() & 1)) as u32).to_le_bytes());
        webp.extend_from_slice(b"WEBPEXIF");
        webp.extend_from_slice(&(tiff.len() as u32).to_le_bytes());
        webp.extend_from_slice(&tiff);
        if tiff.len() & 1 == 1 {
            webp.push(0);
        }
        assert_eq!(Fixture::new(&webp).read().unwrap(), expected);
    }
    #[test]
    fn rejects_oversize_file_and_chunk_lengths() {
        let fixture = Fixture::new(&[]);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&fixture.0)
            .unwrap()
            .set_len(MAX_FILE + 1)
            .unwrap();
        assert!(fixture.read().unwrap_err().contains("512 MiB"));
        let mut bytes = encoded_png();
        bytes[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(Fixture::new(&bytes).read().is_err());
    }
    /// The exact ComfyUI save paths civitai.com's own fixtures mirror: VHS (FFMETADATA file +
    /// use_metadata_tags, moov at end), core SaveVideo mp4 (+faststart), WebM/MKV global tags,
    /// a streamed unknown-size WebM segment, and tag-less files.
    #[test]
    fn video_container_tags_as_comfyui_writes_them() {
        let ffmpeg = |args: &[&str]| {
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc=size=16x16:rate=4:duration=0.5",
                ])
                .args(args)
                .status()
                .ok()
                .filter(|s| s.success())
                .is_some()
        };
        let dir = std::env::temp_dir().join(format!("deas-video-tags-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let prompt = r#"{"5":{"class_type":"KSampler","inputs":{"seed":7}}}"#;
        let workflow = r#"{"nodes":[],"extra":{"ü":"ok"}}"#;
        std::fs::write(
            dir.join("ffmeta.txt"),
            format!(
                ";FFMETADATA1\nprompt={}\nworkflow={}\n",
                prompt
                    .chars()
                    .map(|c| if "=;#\\".contains(c) {
                        format!("\\{c}")
                    } else {
                        c.to_string()
                    })
                    .collect::<String>(),
                workflow
            ),
        )
        .unwrap();
        let h264 = ["-c:v", "libx264", "-pix_fmt", "yuv420p"];
        let tags = [
            "-metadata",
            &format!("prompt={prompt}"),
            "-metadata",
            &format!("workflow={workflow}"),
        ];
        if !ffmpeg(
            &[
                &["-i", &p("ffmeta.txt"), "-map", "0", "-map_metadata", "1"][..],
                &h264[..],
                &["-movflags", "use_metadata_tags", &p("vhs.mp4")],
            ]
            .concat(),
        ) {
            eprintln!("ffmpeg unavailable; video fixture test skipped");
            return;
        }
        // The FFMETADATA escape above turns `=` into `\=`; ffmpeg unescapes it back, so the vhs file carries the same prompt.
        assert!(ffmpeg(
            &[
                &h264[..],
                &tags[..],
                &[
                    "-movflags",
                    "use_metadata_tags+faststart",
                    &p("core-faststart.mp4")
                ]
            ]
            .concat()
        ));
        assert!(ffmpeg(
            &[
                &["-c:v", "libvpx-vp9"][..],
                &tags[..],
                &[p("core.webm").as_str()]
            ]
            .concat()
        ));
        assert!(ffmpeg(
            &[&h264[..], &tags[..], &[p("core.mkv").as_str()]].concat()
        ));
        assert!(ffmpeg(&[
            "-c:v",
            "libvpx-vp9",
            "-metadata",
            &format!("prompt={prompt}"),
            "-live",
            "1",
            "-f",
            "webm",
            &p("live-prompt.webm")
        ]));
        assert!(ffmpeg(&[&h264[..], &[p("plain.mp4").as_str()]].concat()));
        assert!(ffmpeg(&["-c:v", "libvpx-vp9", &p("plain.webm")]));
        let both = vec![
            ("prompt".to_string(), prompt.to_string()),
            ("workflow".to_string(), workflow.to_string()),
        ];
        for name in ["vhs.mp4", "core-faststart.mp4", "core.webm", "core.mkv"] {
            let mut got = read_metadata(&dir.join(name)).unwrap();
            got.sort();
            assert_eq!(got, both, "{name}");
        }
        assert_eq!(
            read_metadata(&dir.join("live-prompt.webm")).unwrap(),
            both[..1]
        );
        for name in ["plain.mp4", "plain.webm"] {
            assert!(read_metadata(&dir.join(name)).unwrap().is_empty(), "{name}");
        }
        let truncated = std::fs::read(dir.join("core.mkv")).unwrap();
        assert!(
            Fixture::new(&truncated[..truncated.len() / 2])
                .read()
                .is_ok()
        ); // tags precede the clusters
        assert!(Fixture::new(b"\x1a\x45\xdf\xa3\x00").read().is_err());
        assert!(
            Fixture::new(b"\0\0\0\x0cftypisom\0\0\0\x02moov")
                .read()
                .is_err()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn exif_description_and_unicode_comment() {
        let mut data = b"II*\0\x08\0\0\0\x02\0".to_vec();
        data.extend_from_slice(&[0x0e, 0x01, 2, 0, 4, 0, 0, 0, b'c', b'a', b't', 0]);
        data.extend_from_slice(&[0x69, 0x87, 4, 0, 1, 0, 0, 0, 38, 0, 0, 0]);
        data.extend_from_slice(&[0; 4]);
        data.extend_from_slice(&[1, 0]);
        data.extend_from_slice(&[0x86, 0x92, 7, 0, 12, 0, 0, 0, 56, 0, 0, 0]);
        data.extend_from_slice(&[0; 4]);
        data.extend_from_slice(b"UNICODE\0\xff\xfe\x2b\x73");
        let mut out = Vec::new();
        exif(&data, &mut out).unwrap();
        assert_eq!(
            out,
            vec![
                ("ImageDescription".into(), "cat".into()),
                ("UserComment".into(), "猫".into())
            ]
        );
    }
}
#[cfg(test)]
mod live {
    /// `DEAS_LIVE_VIDEO=<path> cargo test --lib live_video_tags -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_video_tags() {
        let path = std::env::var("DEAS_LIVE_VIDEO").expect("DEAS_LIVE_VIDEO");
        let tags = super::read_metadata(std::path::Path::new(&path)).unwrap();
        for (k, v) in &tags {
            println!("{k}: {} bytes: {}", v.len(), &v[..v.len().min(160)]);
        }
        assert!(tags.iter().any(|(k, _)| k == "prompt"));
    }
}
