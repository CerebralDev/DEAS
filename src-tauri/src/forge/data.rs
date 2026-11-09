//! Built-in, pinned Forge data; existing environment files override it without writes.
//! The embedded payload is zstd-compressed; a raw override file is read as-is.
use rusqlite::{Connection, OpenFlags};
use std::io::{self, Read};
use std::path::Path;

const MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

pub fn bundled(name: &str) -> Option<&'static [u8]> {
    match name {
        "forge-pack.json" => Some(include_bytes!("../../forge-data/forge-pack.json")),
        "deas-models.json" => Some(include_bytes!("../../forge-data/deas-models.json")),
        "randomizer.json" => Some(include_bytes!("../../forge-data/randomizer.json")),
        "tags.sqlite" => Some(include_bytes!("../../forge-data/tags.sqlite")),
        "character_traits.sqlite" => {
            Some(include_bytes!("../../forge-data/character_traits.sqlite"))
        }
        // Media Analysis' Describe: who else is in a series, and its instructions.
        "character_series.json" => Some(include_bytes!("../../forge-data/character_series.json")),
        "analysis-describe.md" => Some(include_bytes!("../../forge-data/analysis-describe.md")),
        _ => None,
    }
}

// A compressed override or bundle must decode; a raw one is returned unchanged.
fn decompress(bytes: &[u8]) -> io::Result<Vec<u8>> {
    if !bytes.starts_with(&MAGIC) {
        return Ok(bytes.to_vec());
    }
    let mut out = Vec::new();
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    match std::fs::read(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let bytes = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(bundled)
                .ok_or(e)?;
            decompress(bytes)
        }
        result => result.and_then(|bytes| decompress(&bytes)),
    }
}

pub fn database(path: &Path) -> Result<Connection, String> {
    // Only a missing file selects the bundle; malformed/unreadable overrides remain errors.
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let bytes = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(bundled)
                .ok_or_else(|| e.to_string())?;
            let bytes = decompress(bytes).map_err(|e| e.to_string())?;
            let mut db = Connection::open_in_memory().map_err(|e| e.to_string())?;
            db.deserialize_read_exact(rusqlite::MAIN_DB, bytes.as_slice(), bytes.len(), true)
                .map_err(|e| e.to_string())?;
            Ok(db)
        }
        Err(e) => Err(e.to_string()),
        Ok(_) => Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| e.to_string()),
    }
}
