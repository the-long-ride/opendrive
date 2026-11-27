//! System & imported font discovery plus zero-dependency SFNT/OpenType validation and family extraction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFont {
    pub family: String,
    pub path: PathBuf,
    pub imported: bool,
}

fn be_u16(bytes: &[u8], off: usize) -> Option<u16> {
    let slice: [u8; 2] = bytes.get(off..off + 2)?.try_into().ok()?;
    Some(u16::from_be_bytes(slice))
}

fn be_u32(bytes: &[u8], off: usize) -> Option<u32> {
    let slice: [u8; 4] = bytes.get(off..off + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(slice))
}

fn decode_name_record(raw: &[u8], platform: u16) -> Option<String> {
    let s = if platform == 3 || platform == 0 {
        if !raw.len().is_multiple_of(2) {
            return None;
        }
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(u16::from_be_bytes)
            .collect();
        String::from_utf16(&units).ok()?
    } else if platform == 1 {
        raw.iter().map(|&b| b as char).collect()
    } else {
        return None;
    };
    let trimmed = s.trim().trim_matches('\0').trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn parse_sfnt_family(bytes: &[u8], sfnt_off: usize) -> Result<Option<String>, String> {
    let tag = bytes
        .get(sfnt_off..sfnt_off + 4)
        .ok_or_else(|| "Font file is truncated".to_string())?;
    let valid_sfnt = tag == [0, 1, 0, 0] || tag == b"OTTO" || tag == b"true";
    if !valid_sfnt {
        return Err("Unsupported font header (expected TTF, OTF, or TTC)".into());
    }
    let num_tables =
        be_u16(bytes, sfnt_off + 4).ok_or_else(|| "Invalid SFNT table count".to_string())? as usize;
    if num_tables == 0 {
        return Err("Font file contains zero SFNT tables".into());
    }
    let dir_start = sfnt_off + 12;
    let dir_end = dir_start + num_tables * 16;
    if bytes.len() < dir_end {
        return Err("Font table directory is truncated".into());
    }

    let mut name_table = None;
    for idx in 0..num_tables {
        let rec = dir_start + idx * 16;
        if bytes.get(rec..rec + 4) == Some(b"name") {
            let off = be_u32(bytes, rec + 8).unwrap_or(0) as usize;
            let len = be_u32(bytes, rec + 12).unwrap_or(0) as usize;
            name_table = Some((off, len));
            break;
        }
    }
    let Some((name_off, name_len)) = name_table else {
        return Ok(None);
    };
    let Some(tbl) = bytes.get(name_off..name_off.saturating_add(name_len)) else {
        return Ok(None);
    };
    let Some(count) = be_u16(tbl, 2).map(usize::from) else {
        return Ok(None);
    };
    let Some(str_off) = be_u16(tbl, 4).map(usize::from) else {
        return Ok(None);
    };
    let mut win_family = None;
    let mut mac_family = None;
    for i in 0..count {
        let rec = 6 + i * 12;
        let (Some(plat), Some(nid), Some(len), Some(off)) = (
            be_u16(tbl, rec),
            be_u16(tbl, rec + 6),
            be_u16(tbl, rec + 8).map(usize::from),
            be_u16(tbl, rec + 10).map(usize::from),
        ) else {
            break;
        };
        if nid != 1 {
            continue;
        }
        let start = str_off + off;
        let Some(raw) = tbl.get(start..start.saturating_add(len)) else {
            continue;
        };
        if plat == 3 && win_family.is_none() {
            win_family = decode_name_record(raw, plat);
        } else if (plat == 1 || plat == 0) && mac_family.is_none() {
            mac_family = decode_name_record(raw, plat);
        }
    }
    Ok(win_family.or(mac_family))
}

pub fn validate_font_bytes(bytes: &[u8]) -> Result<Option<String>, String> {
    if bytes.len() < 12 {
        return Err("Font file is too small to be a valid TTF/OTF/TTC".into());
    }
    if &bytes[0..4] == b"ttcf" {
        let num_fonts = be_u32(bytes, 8).unwrap_or(0);
        if num_fonts == 0 {
            return Err("TTC font collection contains no fonts".into());
        }
        let first_off = be_u32(bytes, 12).ok_or_else(|| "Truncated TTC header".to_string())?;
        return parse_sfnt_family(bytes, first_off as usize);
    }
    parse_sfnt_family(bytes, 0)
}

pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("WINDIR") {
            dirs.push(PathBuf::from(root).join("Fonts"));
        } else {
            dirs.push(PathBuf::from(r"C:\Windows\Fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(
                PathBuf::from(local)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Fonts"),
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join(".local/share/fonts"));
        }
    }
    dirs
}

fn font_style_rank(path: &Path) -> u8 {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if ["bold", "italic", "oblique", "black", "heavy", "thin"]
        .iter()
        .any(|k| stem.contains(k))
        || ["bd", "bi", "it", "z"].iter().any(|s| stem.ends_with(s))
    {
        return 2;
    }
    if ["light", "medium", "semi", "extra", "condensed", "narrow"]
        .iter()
        .any(|k| stem.contains(k))
    {
        return 1;
    }
    0
}

fn scan_font_dir(dir: &Path, imported: bool, out: &mut Vec<DiscoveredFont>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            scan_font_dir(&path, imported, out);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if !matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            continue;
        }
        let family = std::fs::read(&path)
            .ok()
            .and_then(|b| validate_font_bytes(&b).ok().flatten())
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Font")
                    .to_string()
            });
        out.push(DiscoveredFont {
            family,
            path,
            imported,
        });
    }
}

pub fn discover_system_fonts(imported_dir: Option<&Path>) -> Vec<DiscoveredFont> {
    discover_fonts_in_dirs(&system_font_dirs(), imported_dir)
}

pub fn discover_fonts_in_dirs(
    system_dirs: &[PathBuf],
    imported_dir: Option<&Path>,
) -> Vec<DiscoveredFont> {
    let mut raw = Vec::new();
    if let Some(dir) = imported_dir {
        scan_font_dir(dir, true, &mut raw);
    }
    for dir in system_dirs {
        scan_font_dir(dir, false, &mut raw);
    }
    let mut map: HashMap<String, DiscoveredFont> = HashMap::new();
    for item in raw {
        let key = item.family.to_ascii_lowercase();
        let replace = match map.get(&key) {
            None => true,
            Some(prev) => {
                (item.imported && !prev.imported)
                    || (item.imported == prev.imported
                        && font_style_rank(&item.path) < font_style_rank(&prev.path))
            }
        };
        if replace {
            map.insert(key, item);
        }
    }
    let mut out: Vec<DiscoveredFont> = map.into_values().collect();
    out.sort_by(|a, b| {
        a.family
            .to_ascii_lowercase()
            .cmp(&b.family.to_ascii_lowercase())
    });
    out
}

pub fn resolve_font_bytes(family: &str, fonts: &[DiscoveredFont]) -> Option<Vec<u8>> {
    let target = family.trim();
    if target.is_empty() {
        return None;
    }
    let found = fonts
        .iter()
        .find(|f| f.family.eq_ignore_ascii_case(target))?;
    std::fs::read(&found.path).ok()
}
