//! Windows virtual drive letter mounting via `subst` and Explorer `DriveIcons`.

use crate::{PlatformError, Result};
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

/// Returns the `subst` command and arguments to map `letter:` to `target`.
pub fn drive_mount_command(letter: char, target: &Path) -> (String, Vec<String>) {
    (
        "subst".into(),
        vec![format!("{letter}:"), target.display().to_string()],
    )
}

/// Returns the `subst` command and arguments to unmap `letter:`.
pub fn drive_unmount_command(letter: char) -> (String, Vec<String>) {
    ("subst".into(), vec![format!("{letter}:"), "/D".into()])
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
        if let Some(target) = rest {
            let path = target.trim();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

#[cfg(windows)]
fn run_hidden(program: &str, args: &[&str]) -> Result<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| PlatformError::Other(format!("failed to run {program}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(PlatformError::Other(format!(
            "{program} exited with {}: {detail}",
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(windows)]
pub fn mount_virtual_drive(letter: &str, target: &Path) -> Result<char> {
    let drive = normalize_drive_letter(letter)?;
    std::fs::create_dir_all(target)
        .map_err(|e| PlatformError::Other(format!("create {}: {e}", target.display())))?;

    let listing = run_hidden("subst", &[])?;
    if let Some(existing) = parse_subst_target(&listing, drive) {
        let same = existing == target
            || std::fs::canonicalize(&existing).ok() == std::fs::canonicalize(target).ok();
        if !same {
            return Err(PlatformError::Other(format!(
                "drive {drive}: is already mapped to {}",
                existing.display()
            )));
        }
    } else {
        let (prog, args) = drive_mount_command(drive, target);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_hidden(&prog, &refs)?;
    }

    let key = format!(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\DriveIcons\{drive}\DefaultLabel"
    );
    let _ = run_hidden("reg", &["add", &key, "/ve", "/d", "OpenDrive", "/f"]);
    if let Some(ico) = crate::data_dir()
        .ok()
        .map(|d| d.join("opendrive.ico"))
        .filter(|p| p.is_file())
    {
        let icon_key = format!(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\DriveIcons\{drive}\DefaultIcon"
        );
        let icon_val = format!("{},0", ico.display());
        let _ = run_hidden("reg", &["add", &icon_key, "/ve", "/d", &icon_val, "/f"]);
    }
    Ok(drive)
}

#[cfg(windows)]
pub fn unmount_virtual_drive(letter: &str) -> Result<()> {
    let drive = normalize_drive_letter(letter)?;
    let listing = run_hidden("subst", &[])?;
    if parse_subst_target(&listing, drive).is_some() {
        let (prog, args) = drive_unmount_command(drive);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_hidden(&prog, &refs)?;
    }
    let key =
        format!(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\DriveIcons\{drive}");
    let _ = run_hidden("reg", &["delete", &key, "/f"]);
    Ok(())
}

#[cfg(not(windows))]
pub fn mount_virtual_drive(_letter: &str, _target: &Path) -> Result<char> {
    Err(PlatformError::Other(
        "virtual drive letters are only supported on Windows".into(),
    ))
}

#[cfg(not(windows))]
pub fn unmount_virtual_drive(_letter: &str) -> Result<()> {
    Err(PlatformError::Other(
        "virtual drive letters are only supported on Windows".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drive_letter_accepts_valid_and_rejects_invalid() {
        assert_eq!(normalize_drive_letter("o").unwrap(), 'O');
        assert_eq!(normalize_drive_letter("  z:\\ ").unwrap(), 'Z');
        assert_eq!(normalize_drive_letter("D:/").unwrap(), 'D');
        assert!(normalize_drive_letter("").is_err());
        assert!(normalize_drive_letter("CD").is_err());
        assert!(normalize_drive_letter("1:").is_err());
        assert!(normalize_drive_letter("A:").is_err());
        assert!(normalize_drive_letter("b").is_err());
        assert!(normalize_drive_letter("C:\\").is_err());
        assert_eq!(is_virtual_drive_supported(), cfg!(windows));
    }

    #[test]
    fn drive_commands_and_subst_parsing() {
        let target = Path::new(r"C:\Users\Alice\OpenDrive");
        let (prog, args) = drive_mount_command('O', target);
        assert_eq!(prog, "subst");
        assert_eq!(args, vec!["O:", r"C:\Users\Alice\OpenDrive"]);

        let (uprog, uargs) = drive_unmount_command('O');
        assert_eq!(uprog, "subst");
        assert_eq!(uargs, vec!["O:", "/D"]);

        let stdout = "D:\\: => C:\\Work\no:\\: =>   C:\\Users\\Alice\\OpenDrive  \n";
        assert_eq!(
            parse_subst_target(stdout, 'O'),
            Some(PathBuf::from(r"C:\Users\Alice\OpenDrive"))
        );
        assert_eq!(
            parse_subst_target(stdout, 'd'),
            Some(PathBuf::from(r"C:\Work"))
        );
        assert_eq!(parse_subst_target(stdout, 'Z'), None);
    }
}
