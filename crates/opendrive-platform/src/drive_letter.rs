//! Windows drive letter validation, auto-selection, `subst` parsing, and registry command builders.

use crate::{format_explorer_quota_label, ExplorerQuota, PlatformError, Result};
use std::path::{Path, PathBuf};

/// Returns `true` on Windows where virtual drive letter mapping is supported.
pub fn is_virtual_drive_supported() -> bool {
    cfg!(windows)
}

/// Validates and normalizes a Windows drive letter input (e.g. `"o:"`, `" O:\\ "` -> `'O'`).
pub fn normalize_drive_letter(raw: &str) -> Result<char> {
    let cleaned = raw.trim().trim_end_matches([':', '\\', '/']).trim();
    let mut chars = cleaned.chars();
    let letter = match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => c.to_ascii_uppercase(),
        _ => {
            return Err(PlatformError::Other(format!(
                "invalid drive letter '{raw}': expected a single ASCII letter D-Z"
            )))
        }
    };
    if matches!(letter, 'A' | 'B' | 'C') {
        return Err(PlatformError::Other(format!(
            "drive letter '{letter}:' is reserved; choose D-Z"
        )));
    }
    Ok(letter)
}

pub(crate) fn paths_match(a: &Path, b: &Path) -> bool {
    if a == b
        || a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    {
        return true;
    }
    std::fs::canonicalize(a)
        .ok()
        .zip(std::fs::canonicalize(b).ok())
        .is_some_and(|(ca, cb)| ca == cb)
}

/// Picks `preferred` when free or already mapped to `target`; otherwise falls back across `O..=Z` then `D..=N`.
pub fn pick_available_drive_letter(
    preferred: &str,
    subst_stdout: &str,
    target: &Path,
    is_drive_in_use: impl Fn(char) -> bool,
) -> Result<char> {
    let pref = normalize_drive_letter(preferred)?;
    if let Some(existing) = parse_subst_target(subst_stdout, pref) {
        if paths_match(&existing, target) {
            return Ok(pref);
        }
    } else if !is_drive_in_use(pref) {
        return Ok(pref);
    }
    for cand in ('O'..='Z').chain('D'..='N') {
        if cand == pref {
            continue;
        }
        if let Some(existing) = parse_subst_target(subst_stdout, cand) {
            if paths_match(&existing, target) {
                return Ok(cand);
            }
        } else if !is_drive_in_use(cand) {
            return Ok(cand);
        }
    }
    Err(PlatformError::Other("no available drive letter D-Z".into()))
}

pub fn drive_mount_command(letter: char, target: &Path) -> (String, Vec<String>) {
    (
        "subst".into(),
        vec![format!("{letter}:"), target.display().to_string()],
    )
}

pub fn drive_unmount_command(letter: char) -> (String, Vec<String>) {
    ("subst".into(), vec![format!("{letter}:"), "/D".into()])
}

fn drive_icons_reg_add(drive: char, subkey: &str, value: String) -> (String, Vec<String>) {
    let key = format!(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\DriveIcons\{drive}\{subkey}"
    );
    (
        "reg".into(),
        vec![
            "add".into(),
            key,
            "/ve".into(),
            "/d".into(),
            value,
            "/f".into(),
        ],
    )
}

pub fn drive_label_reg_command(drive: char, quota: Option<ExplorerQuota>) -> (String, Vec<String>) {
    drive_icons_reg_add(drive, "DefaultLabel", format_explorer_quota_label(quota))
}

pub fn drive_icon_reg_command(drive: char, icon_path: &Path) -> (String, Vec<String>) {
    drive_icons_reg_add(drive, "DefaultIcon", format!("{},0", icon_path.display()))
}

/// Parses `subst` stdout to find the mapped target path for `letter` (case-insensitive).
pub fn parse_subst_target(stdout: &str, letter: char) -> Option<PathBuf> {
    let upper = format!("{}:\\: => ", letter.to_ascii_uppercase());
    let lower = format!("{}:\\: => ", letter.to_ascii_lowercase());
    for line in stdout.lines() {
        let trimmed = line.trim();
        let rest = trimmed
            .strip_prefix(&upper)
            .or_else(|| trimmed.strip_prefix(&lower));
        if let Some(path) = rest.map(str::trim).filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(path));
        }
    }
    None
}
