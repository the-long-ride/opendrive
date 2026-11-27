//! Windows virtual drive letter mounting via `subst`, `DriveIcons`, and Shell drive notifications.

use crate::drive_letter::normalize_drive_letter;
#[cfg(windows)]
use crate::drive_letter::{
    drive_icon_reg_command, drive_label_reg_command, drive_mount_command, drive_unmount_command,
    parse_subst_target, paths_match, pick_available_drive_letter,
};
use crate::{ExplorerQuota, PlatformError, Result};
use std::path::Path;

#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn SHChangeNotify(
        event: i32,
        flags: u32,
        item1: *const core::ffi::c_void,
        item2: *const core::ffi::c_void,
    );
}

#[cfg(windows)]
fn notify_drive_changed(drive: char, added: bool) {
    let event = if added { 0x0000_0100 } else { 0x0000_0080 };
    let root = format!("{drive}:\\\0");
    let wide: Vec<u16> = root.encode_utf16().collect();
    unsafe {
        SHChangeNotify(event, 0x0005, wide.as_ptr().cast(), core::ptr::null());
    }
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
pub fn mount_virtual_drive_with_quota(
    letter: &str,
    target: &Path,
    quota: Option<ExplorerQuota>,
) -> Result<char> {
    let listing = run_hidden("subst", &[]).unwrap_or_default();
    let chosen = pick_available_drive_letter(letter, &listing, target, |c| {
        Path::new(&format!("{c}:\\")).exists()
    })?;
    let ico = crate::data_dir().ok().map(|d| d.join("opendrive.ico"));
    let drive = mount_with_runner(
        &chosen.to_string(),
        target,
        quota,
        ico.as_deref(),
        run_hidden,
    )?;
    notify_drive_changed(drive, true);
    crate::notify_shell_mounts_changed();
    Ok(drive)
}

#[cfg(windows)]
pub fn mount_virtual_drive(letter: &str, target: &Path) -> Result<char> {
    mount_virtual_drive_with_quota(letter, target, None)
}

#[cfg(not(windows))]
pub fn mount_virtual_drive_with_quota(
    _letter: &str,
    _target: &Path,
    _quota: Option<ExplorerQuota>,
) -> Result<char> {
    Err(PlatformError::Other(
        "virtual drive letters are only supported on Windows".into(),
    ))
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
) -> Result<bool> {
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
    Ok(should_unmount)
}

#[cfg(windows)]
pub fn unmount_virtual_drive(letter: &str) -> Result<()> {
    let drive = normalize_drive_letter(letter)?;
    let _ = unmount_with_runner(letter, None, run_hidden)?;
    notify_drive_changed(drive, false);
    crate::notify_shell_mounts_changed();
    Ok(())
}

/// Unmounts `letter:` only when `subst` maps it to `target`, preserving unrelated drive letters.
#[cfg(windows)]
pub fn unmount_virtual_drive_for_target(letter: &str, target: &Path) -> Result<()> {
    let drive = normalize_drive_letter(letter)?;
    if unmount_with_runner(letter, Some(target), run_hidden)? {
        notify_drive_changed(drive, false);
        crate::notify_shell_mounts_changed();
    }
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
        let q_test = ExplorerQuota {
            used: 100,
            total: 1000,
            free: 900,
        };
        let m_ico = mount_with_runner("O", &target, Some(q_test), Some(&ico), |p, a| {
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
