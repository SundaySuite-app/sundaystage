//! Phase 7.2 — media file helpers (the path-stability core).
//!
//! ProPresenter and OpenLP both fall over when media files move. We fix that
//! by fingerprinting content on import and, when a stored path goes missing,
//! searching common locations for a file with the same fingerprint and
//! auto-relinking.
//!
//! The fingerprint is **O(1)** — `(size, first 64 KiB, last 64 KiB)` hashed —
//! the same approach the sibling SundayEdit app uses. Hashing whole multi-GB
//! videos on every import or relink scan would be far too slow; head+tail+size
//! is plenty to distinguish real-world media without reading the entire file.
//! It is a fast content fingerprint, **not** a cryptographic hash.
//!
//! Thumbnail generation (ffmpeg) and resolution/duration probing (ffprobe) are
//! deferred — they need ffmpeg on the box, which isn't available in this
//! headless environment. The model leaves nullable columns for them.

use std::collections::hash_map::DefaultHasher;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::models::MediaAsset;

/// A media asset plus whether its `original_path` currently exists on disk —
/// the browser shows a "broken, relink?" badge when `present` is false.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/MediaStatus.ts")]
pub struct MediaStatus {
    pub asset: MediaAsset,
    pub present: bool,
}

const CHUNK: usize = 64 * 1024;
const MAX_WALK_DEPTH: usize = 6;

/// Read up to `buf.len()` bytes, returning how many were read (handles short
/// reads without erroring at EOF).
fn read_up_to(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match f.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

/// Fast content fingerprint: hash of `(len, head chunk, tail chunk)`.
pub fn content_fingerprint(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();

    let mut hasher = DefaultHasher::new();
    len.hash(&mut hasher);

    let mut buf = vec![0u8; CHUNK];
    let n = read_up_to(&mut f, &mut buf)?;
    buf[..n].hash(&mut hasher);

    // Distinct tail chunk only when the file is larger than one chunk.
    if len > CHUNK as u64 {
        f.seek(SeekFrom::End(-(CHUNK as i64)))?;
        let n = read_up_to(&mut f, &mut buf)?;
        buf[..n].hash(&mut hasher);
    }

    Ok(format!("{:016x}", hasher.finish()))
}

/// Search `dirs` (recursively, depth-bounded) for a file whose fingerprint
/// matches `fingerprint`. Returns the first match — used to auto-relink a moved
/// asset. Unreadable files/dirs are skipped, never fatal.
pub fn find_by_fingerprint(fingerprint: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in dirs {
        if let Some(found) = walk(dir, fingerprint, 0) {
            return Some(found);
        }
    }
    None
}

fn walk(dir: &Path, fingerprint: &str, depth: usize) -> Option<PathBuf> {
    if depth > MAX_WALK_DEPTH {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = walk(&path, fingerprint, depth + 1) {
                return Some(found);
            }
        } else if let Ok(fp) = content_fingerprint(&path) {
            if fp == fingerprint {
                return Some(path);
            }
        }
    }
    None
}

/// Classify a file by extension into the schema's `kind` domain. `None` for
/// unsupported types (the importer rejects those).
pub fn detect_kind(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" | "avif" => Some("image"),
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" => Some("video"),
        "mp3" | "wav" | "aac" | "flac" | "ogg" | "m4a" => Some("audio"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        let mut f = File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn identical_content_yields_identical_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "a.png", b"the same bytes everywhere");
        let b = write_file(dir.path(), "b.png", b"the same bytes everywhere");
        assert_eq!(
            content_fingerprint(&a).unwrap(),
            content_fingerprint(&b).unwrap(),
        );
    }

    #[test]
    fn different_content_yields_different_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "a.png", b"first");
        let b = write_file(dir.path(), "b.png", b"second is different");
        assert_ne!(
            content_fingerprint(&a).unwrap(),
            content_fingerprint(&b).unwrap(),
        );
    }

    #[test]
    fn fingerprint_handles_files_larger_than_one_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let big: Vec<u8> = (0..(CHUNK * 3)).map(|i| (i % 251) as u8).collect();
        let a = write_file(dir.path(), "big.mp4", &big);
        let fp1 = content_fingerprint(&a).unwrap();
        // Same bytes elsewhere → same fingerprint.
        let b = write_file(dir.path(), "copy.mp4", &big);
        assert_eq!(fp1, content_fingerprint(&b).unwrap());
    }

    #[test]
    fn relink_finds_a_moved_file_by_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let original = write_file(
            dir.path(),
            "background.png",
            b"church background image bytes",
        );
        let fp = content_fingerprint(&original).unwrap();

        // "Move" it into a nested subfolder.
        let sub = dir.path().join("nested").join("deeper");
        std::fs::create_dir_all(&sub).unwrap();
        let moved = sub.join("background.png");
        std::fs::rename(&original, &moved).unwrap();

        let found = find_by_fingerprint(&fp, &[dir.path().to_path_buf()]).unwrap();
        assert_eq!(found, moved);
    }

    #[test]
    fn relink_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "x.png", b"abc");
        assert!(find_by_fingerprint("deadbeefdeadbeef", &[dir.path().to_path_buf()]).is_none());
    }

    #[test]
    fn detect_kind_by_extension() {
        assert_eq!(detect_kind(Path::new("a.PNG")), Some("image"));
        assert_eq!(detect_kind(Path::new("clip.mp4")), Some("video"));
        assert_eq!(detect_kind(Path::new("track.mp3")), Some("audio"));
        assert_eq!(detect_kind(Path::new("notes.txt")), None);
        assert_eq!(detect_kind(Path::new("noext")), None);
    }
}
