//! Windows virtual drive letter mounting via `subst` and Explorer `DriveIcons`.

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

/// Returns the `reg` command and arguments to set `DriveIcons\{drive}\DefaultLabel`.
pub fn drive_label_reg_command(drive: char, quota: Option<ExplorerQuota>) -> (String, Vec<String>) {
    drive_icons_reg_add(drive, "DefaultLabel", format_explorer_quota_label(quota))
}

/// Returns the `reg` command and arguments to set `DriveIcons\{drive}\DefaultIcon`.
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

#[cfg(windows)]
pub(crate) fn run_hidden(program: &str, args: &[&str]) -> Result<String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new(program)
        .args(args)
        .creation_flags(0x0800_0000)
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
    let ico = crate::data_dir().ok().map(|d| d.join("opendrive.ico"));
    let drive = mount_with_runner(letter, target, None, ico.as_deref(), run_hidden)?;
    crate::notify_shell_mounts_changed();
    Ok(drive)
}

#[cfg(not(windows))]
pub fn mount_virtual_drive(_letter: &str, _target: &Path) -> Result<char> {
    Err(PlatformError::Other(
        "virtual drive letters are only supported on Windows".into(),
    ))
}

#[cfg(windows)]
fn run_owned_cmd(
    run: &mut impl FnMut(&str, &[&str]) -> Result<String>,
    prog: &str,
    args: &[String],
) -> Result<String> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(prog, &refs)
}

#[cfg(windows)]
fn paths_match(a: &Path, b: &Path) -> bool {
    a == b
        || std::fs::canonicalize(a)
            .ok()
            .zip(std::fs::canonicalize(b).ok())
            .is_some_and(|(ca, cb)| ca == cb)
}

#[cfg(windows)]
fn mount_with_runner(
    letter: &str,
    target: &Path,
    quota: Option<ExplorerQuota>,
    icon: Option<&Path>,
    mut run: impl FnMut(&str, &[&str]) -> Result<String>,
) -> Result<char> {
    let drive = normalize_drive_letter(letter)?;
    std::fs::create_dir_all(target)
        .map_err(|e| PlatformError::Other(format!("create {}: {e}", target.display())))?;
    let listing = run("subst", &[])?;
    if let Some(existing) = parse_subst_target(&listing, drive) {
        if !paths_match(&existing, target) {
            return Err(PlatformError::Other(format!(
                "drive {drive}: is already mapped to {}",
                existing.display()
            )));
        }
    } else {
        let (prog, args) = drive_mount_command(drive, target);
        run_owned_cmd(&mut run, &prog, &args)?;
    }
    let (lprog, largs) = drive_label_reg_command(drive, quota);
    let _ = run_owned_cmd(&mut run, &lprog, &largs);
    if let Some(ico) = icon.filter(|p| p.is_file()) {
        let (iprog, iargs) = drive_icon_reg_command(drive, ico);
        let _ = run_owned_cmd(&mut run, &iprog, &iargs);
    }
    Ok(drive)
}

#[cfg(windows)]
fn unmount_with_runner(
    letter: &str,
    only_target: Option<&Path>,
    mut run: impl FnMut(&str, &[&str]) -> Result<String>,
) -> Result<()> {
    let drive = normalize_drive_letter(letter)?;
    let listing = run("subst", &[])?;
    let should_unmount = match (parse_subst_target(&listing, drive), only_target) {
        (Some(_), None) => true,
        (Some(existing), Some(target)) => paths_match(&existing, target),
        (None, _) => false,
    };
    if should_unmount {
        let (prog, args) = drive_unmount_command(drive);
        run_owned_cmd(&mut run, &prog, &args)?;
    }
    if should_unmount || only_target.is_none() {
        let key =
            format!(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\DriveIcons\{drive}");
        let _ = run("reg", &["delete", &key, "/f"]);
    }
    Ok(())
}

#[cfg(windows)]
pub fn unmount_virtual_drive(letter: &str) -> Result<()> {
    unmount_with_runner(letter, None, run_hidden)?;
    crate::notify_shell_mounts_changed();
    Ok(())
}

/// Unmounts `letter:` only when `subst` maps it to `target`, preserving unrelated drive letters.
#[cfg(windows)]
pub fn unmount_virtual_drive_for_target(letter: &str, target: &Path) -> Result<()> {
    unmount_with_runner(letter, Some(target), run_hidden)?;
    crate::notify_shell_mounts_changed();
    Ok(())
}

#[cfg(not(windows))]
pub fn unmount_virtual_drive(_letter: &str) -> Result<()> {
    Err(PlatformError::Other(
        "virtual drive letters are only supported on Windows".into(),
    ))
}

/// Unmounts `letter:` only when `subst` maps it to `target` (Windows-only).
#[cfg(not(windows))]
pub fn unmount_virtual_drive_for_target(letter: &str, _target: &Path) -> Result<()> {
    normalize_drive_letter(letter)?;
    Err(PlatformError::Other(
        "virtual drive letters are only supported on Windows".into(),
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn mount_unmount_runner_and_run_hidden_branches() {
        let dir = std::env::temp_dir().join(format!("od-dm-{}", std::process::id()));
        let (target, ico) = (dir.join("root"), dir.join("a.ico"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&ico, b"ico").unwrap();
        let mut calls = Vec::new();
        let m_ico = mount_with_runner("O", &target, None, Some(&ico), |p, a| {
            calls.push(format!("{p}:{}", a.join(" ")));
            Ok(String::new())
        });
        assert_eq!(m_ico.unwrap(), 'O');
        assert!(calls.iter().any(|c| c.starts_with("subst:O:")));
        assert!(calls.iter().any(|c| c.starts_with("reg:add")));

        let same = format!("O:\\: => {}\n", target.display());
        let ok = |s: &str| {
            let s = s.to_string();
            move |_: &str, _: &[&str]| Ok(s.clone())
        };
        assert_eq!(
            mount_with_runner("O", &target, None, None, ok(&same)).unwrap(),
            'O'
        );
        assert!(mount_with_runner("O", &target, None, None, ok("O:\\: => C:\\Other\n")).is_err());

        for only in [None, Some(target.as_path())] {
            let mut log = Vec::new();
            unmount_with_runner("O", only, |p, a| {
                log.push(format!("{p}:{}", a.join(" ")));
                Ok(same.clone())
            })
            .unwrap();
            assert!(log.iter().any(|c| c.contains("subst:O: /D")));
        }
        unmount_with_runner("O", Some(&target), ok("O:\\: => C:\\Other\n")).unwrap();

        for (p, a, exp) in [
            ("cmd", &["/C", "echo ok"][..], true),
            ("cmd", &["/C", "echo fail 1>&2 && exit /b 1"][..], false),
            ("cmd", &["/C", "echo outfail && exit /b 1"][..], false),
            ("opendrive-missing-bin-xyz", &[][..], false),
        ] {
            assert_eq!(run_hidden(p, a).is_ok(), exp);
        }
        let fail = |_: &str, _: &[&str]| Err(PlatformError::Other("e".into()));
        fn fcmd(l: String) -> impl FnMut(&str, &[&str]) -> Result<String> {
            move |p, a| {
                if a.is_empty() {
                    Ok(l.clone())
                } else {
                    Err(PlatformError::Other(p.into()))
                }
            }
        }
        assert!(mount_with_runner("O", &target, None, None, fail).is_err());
        assert!(mount_with_runner("O", &target, None, None, fcmd(String::new())).is_err());
        assert!(unmount_with_runner("O", None, fail).is_err());
        assert!(unmount_with_runner("O", None, fcmd(same)).is_err());
        assert!(paths_match(&target, &dir.join(".").join("root")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
