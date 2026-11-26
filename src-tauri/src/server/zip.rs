//! Streaming ZIP export of the shared folders.
//!
//! Entries are stored, not deflated: a LAN link is the bottleneck, not the CPU,
//! and storing keeps `Content-Length` exact without buffering anything. Sizes are
//! known up front from the file metadata, so each entry carries a data descriptor
//! with the CRC that is computed while the bytes go out.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{DateTime, Datelike, Local, Timelike};

/// One file over 4 GiB would need zip64 local headers; refuse instead of
/// producing an archive that only some tools can read.
pub const MAX_ENTRY: u64 = u32::MAX as u64;
/// A single request must not be able to walk an absurd tree.
const MAX_ENTRIES: usize = 50_000;
/// Enough for every realistic folder on a LAN; keeps Content-Length sane.
pub const MAX_TOTAL: u64 = 32 * 1024 * 1024 * 1024;

#[derive(Debug)]
pub enum ZipError {
    TooLarge,
    TooManyEntries,
    Unreadable,
}

impl ZipError {
    pub fn status(&self) -> u16 {
        match self {
            ZipError::TooLarge | ZipError::TooManyEntries => 413,
            ZipError::Unreadable => 404,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            ZipError::TooLarge => "the folder is too large to send as one archive",
            ZipError::TooManyEntries => "the folder has too many entries for one archive",
            ZipError::Unreadable => "the folder cannot be read",
        }
    }
}

struct Entry {
    /// Path inside the archive, always with forward slashes; directories end in '/'.
    name: String,
    source: Option<PathBuf>,
    size: u64,
    dos_date: u16,
    dos_time: u16,
    offset: u64,
}

pub struct Plan {
    entries: Vec<Entry>,
    content_length: u64,
}

impl Plan {
    /// Walk `roots` the same way the listing does: hidden names are skipped,
    /// symlinks are ignored so an archive cannot escape a share.
    pub fn build(roots: &[&Path]) -> Result<Self, ZipError> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for root in roots {
            walk(root, "", 0, &mut entries, &mut seen)?;
        }
        Ok(finalize(entries))
    }

    /// Only the named files from the current folder, in the order the listing
    /// shows them. Anything that is not a plain file inside a share is skipped.
    pub fn files(roots: &[&Path], names: &[String]) -> Result<Self, ZipError> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for name in names {
            if name.is_empty() || name.starts_with('.') || name.contains('/') || name.contains('\\')
            {
                continue;
            }
            let Some(path) = roots.iter().map(|root| root.join(name)).find(|path| {
                fs::symlink_metadata(path)
                    .map(|meta| meta.is_file() && !meta.file_type().is_symlink())
                    .unwrap_or(false)
            }) else {
                continue;
            };
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            if meta.len() >= MAX_ENTRY {
                return Err(ZipError::TooLarge);
            }
            if entries.len() >= MAX_ENTRIES {
                return Err(ZipError::TooManyEntries);
            }
            push(
                &mut entries,
                &mut seen,
                name.clone(),
                Some(path),
                meta.len(),
                meta.modified().ok(),
            );
        }
        Ok(finalize(entries))
    }

    pub fn content_length(&self) -> u64 {
        self.content_length
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.name.as_str())
    }

    /// Streams the archive. The plan keeps the CRCs that were computed on the way
    /// so the central directory can be written last.
    pub fn write(&mut self, out: &mut impl Write) -> io::Result<()> {
        let mut crcs = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            out.write_all(&local_header(entry))?;
            let mut crc = crc32_start();
            let mut written = 0u64;
            if let Some(source) = &entry.source {
                let mut file = File::open(source)?;
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    crc = crc32_update(crc, &buffer[..read]);
                    written += read as u64;
                    out.write_all(&buffer[..read])?;
                }
            }
            let crc = crc32_finish(crc);
            crcs.push(crc);
            out.write_all(&data_descriptor(crc, written))?;
        }

        let cd_offset: u64 = self
            .entries
            .last()
            .map(|entry| entry.offset + local_header_len(entry) + entry.size + descriptor_len())
            .unwrap_or(0);
        for (entry, crc) in self.entries.iter().zip(&crcs) {
            out.write_all(&central_header(entry, *crc))?;
        }
        let cd_len: u64 = self.entries.iter().map(central_len).sum();
        if cd_offset >= u32::MAX as u64 || self.entries.len() > u16::MAX as usize {
            out.write_all(&zip64_eocd(self.entries.len(), cd_len, cd_offset))?;
        }
        out.write_all(&eocd(self.entries.len(), cd_len, cd_offset))?;
        Ok(())
    }
}

fn finalize(mut entries: Vec<Entry>) -> Plan {
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let mut offset = 0u64;
    for entry in &mut entries {
        entry.offset = offset;
        offset += local_header_len(entry) + entry.size + descriptor_len();
    }
    let cd_len = entries.iter().map(central_len).sum::<u64>();
    let zip64 = offset >= u32::MAX as u64 || entries.len() > u16::MAX as usize;
    let tail = cd_len + if zip64 { ZIP64_EOCD_LEN } else { 0 } + EOCD_LEN;
    Plan {
        entries,
        content_length: offset + tail,
    }
}

fn walk(
    dir: &Path,
    prefix: &str,
    depth: usize,
    entries: &mut Vec<Entry>,
    seen: &mut std::collections::HashMap<String, u32>,
) -> Result<(), ZipError> {
    if depth > 32 {
        return Ok(());
    }
    let read = fs::read_dir(dir).map_err(|_| ZipError::Unreadable)?;
    let mut children: Vec<_> = read.flatten().collect();
    children.sort_by_key(|entry| entry.file_name());

    for child in children {
        let name = child.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(child.path()) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if entries.len() >= MAX_ENTRIES {
            return Err(ZipError::TooManyEntries);
        }
        let inner = format!("{prefix}{name}");
        if meta.is_dir() {
            push(
                entries,
                seen,
                format!("{inner}/"),
                None,
                0,
                meta.modified().ok(),
            );
            walk(
                &child.path(),
                &format!("{inner}/"),
                depth + 1,
                entries,
                seen,
            )?;
        } else if meta.is_file() {
            if meta.len() >= MAX_ENTRY {
                return Err(ZipError::TooLarge);
            }
            push(
                entries,
                seen,
                inner,
                Some(child.path()),
                meta.len(),
                meta.modified().ok(),
            );
        }
    }
    Ok(())
}

fn push(
    entries: &mut Vec<Entry>,
    seen: &mut std::collections::HashMap<String, u32>,
    name: String,
    source: Option<PathBuf>,
    size: u64,
    modified: Option<std::time::SystemTime>,
) {
    let count = seen.entry(name.clone()).or_insert(0);
    *count += 1;
    let unique = if *count == 1 {
        name
    } else {
        let (stem, ext) = split_extension(&name);
        format!("{stem} ({count}){ext}")
    };
    let (dos_date, dos_time) = dos_stamp(modified);
    entries.push(Entry {
        name: unique,
        source,
        size,
        dos_date,
        dos_time,
        offset: 0,
    });
}

fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(index) if index > 0 => (&name[..index], &name[index..]),
        _ => (name, ""),
    }
}

fn dos_stamp(modified: Option<std::time::SystemTime>) -> (u16, u16) {
    let time = modified
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .and_then(|secs| DateTime::from_timestamp(secs, 0))
        .unwrap_or_else(|| DateTime::<Local>::from(std::time::SystemTime::now()).into())
        .with_timezone(&Local);
    let date = ((time.year().clamp(1980, 2107) - 1980) as u16) << 9
        | (time.month() as u16) << 5
        | time.day() as u16;
    let clock =
        (time.hour() as u16) << 11 | (time.minute() as u16) << 5 | (time.second() as u16 / 2);
    (date, clock)
}

const FLAGS: u16 = 0x0008 | 0x0800; // data descriptor, UTF-8 names

fn local_header_len(entry: &Entry) -> u64 {
    30 + entry.name.len() as u64
}

fn central_len(entry: &Entry) -> u64 {
    let zip64 = entry.size >= u32::MAX as u64 || entry.offset >= u32::MAX as u64;
    46 + entry.name.len() as u64 + if zip64 { 4 + 16 } else { 0 }
}

fn descriptor_len() -> u64 {
    16
}

fn local_header(entry: &Entry) -> Vec<u8> {
    let mut out = Vec::with_capacity(local_header_len(entry) as usize);
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&FLAGS.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // stored
    out.extend_from_slice(&entry.dos_time.to_le_bytes());
    out.extend_from_slice(&entry.dos_date.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // crc, in the descriptor
    out.extend_from_slice(&0u32.to_le_bytes()); // size, in the descriptor
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // no extra field
    out.extend_from_slice(entry.name.as_bytes());
    out
}

fn data_descriptor(crc: u32, size: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&0x0807_4b50u32.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out
}

fn central_header(entry: &Entry, crc: u32) -> Vec<u8> {
    let zip64 = entry.size >= u32::MAX as u64 || entry.offset >= u32::MAX as u64;
    let mut out = Vec::with_capacity(central_len(entry) as usize);
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    out.extend_from_slice(&45u16.to_le_bytes()); // version made by
    out.extend_from_slice(&45u16.to_le_bytes()); // version needed
    out.extend_from_slice(&FLAGS.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&entry.dos_time.to_le_bytes());
    out.extend_from_slice(&entry.dos_date.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    let size32 = if entry.size >= u32::MAX as u64 {
        u32::MAX
    } else {
        entry.size as u32
    };
    let offset32 = if entry.offset >= u32::MAX as u64 {
        u32::MAX
    } else {
        entry.offset as u32
    };
    out.extend_from_slice(&size32.to_le_bytes());
    out.extend_from_slice(&size32.to_le_bytes());
    out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
    out.extend_from_slice(&(if zip64 { 20u16 } else { 0 }).to_le_bytes()); // extra length
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out.extend_from_slice(&0u16.to_le_bytes()); // disk
    out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
    out.extend_from_slice(&offset32.to_le_bytes());
    out.extend_from_slice(entry.name.as_bytes());
    if zip64 {
        out.extend_from_slice(&0x0001u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(&entry.size.to_le_bytes());
        out.extend_from_slice(&entry.offset.to_le_bytes());
    }
    out
}

const ZIP64_EOCD_LEN: u64 = 56 + 20;
const EOCD_LEN: u64 = 22;

fn zip64_eocd(count: usize, cd_len: u64, cd_offset: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(ZIP64_EOCD_LEN as usize);
    out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
    out.extend_from_slice(&44u64.to_le_bytes()); // size of the remaining record
    out.extend_from_slice(&45u16.to_le_bytes());
    out.extend_from_slice(&45u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(count as u64).to_le_bytes());
    out.extend_from_slice(&(count as u64).to_le_bytes());
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    // locator
    out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(cd_offset + cd_len).to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out
}

fn eocd(count: usize, cd_len: u64, cd_offset: u64) -> Vec<u8> {
    let count16 = count.min(u16::MAX as usize) as u16;
    let mut out = Vec::with_capacity(EOCD_LEN as usize);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count16.to_le_bytes());
    out.extend_from_slice(&count16.to_le_bytes());
    out.extend_from_slice(&(cd_len.min(u32::MAX as u64) as u32).to_le_bytes());
    out.extend_from_slice(&(cd_offset.min(u32::MAX as u64) as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn crc32_start() -> u32 {
    0xFFFF_FFFF
}

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for byte in data {
        crc = CRC_TABLE[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc
}

fn crc32_finish(crc: u32) -> u32 {
    crc ^ 0xFFFF_FFFF
}

static CRC_TABLE: [u32; 256] = build_crc_table();

const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                0xEDB8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dos_stamp_is_in_range() {
        let (date, _) = dos_stamp(None);
        assert!(date >> 9 >= 45);
    }

    #[test]
    fn crc_matches_known_value() {
        let crc = crc32_finish(crc32_update(crc32_start(), b"123456789"));
        assert_eq!(crc, 0xCBF4_3926);
    }

    #[test]
    fn extensions_split_for_duplicates() {
        assert_eq!(split_extension("a.txt"), ("a", ".txt"));
        assert_eq!(split_extension("noext"), ("noext", ""));
        assert_eq!(split_extension(".hidden"), (".hidden", ""));
    }
}
