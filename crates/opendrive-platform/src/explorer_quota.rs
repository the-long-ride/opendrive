//! Explorer quota formatting, Windows This PC namespace registration, and app launcher helpers.

use crate::{open_path, PlatformError, Result};
use std::path::Path;
use std::process::{Command, Stdio};

const FILE_SYSTEM_FOLDER_CLSID: &str = "{0E5AAE11-A475-4c5b-AB00-C66DE400274E}";
#[cfg(windows)]
const OPENDRIVE_NAMESPACE_CLSID: &str = "{D84E7B21-6F4A-4D89-9C1E-5A2B3C4D5E6F}";
const CLSID_KEY: &str = r"HKCU\Software\Classes\CLSID\{D84E7B21-6F4A-4D89-9C1E-5A2B3C4D5E6F}";
const NAMESPACE_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpace\{D84E7B21-6F4A-4D89-9C1E-5A2B3C4D5E6F}";
const DESKTOP_NAMESPACE_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\{D84E7B21-6F4A-4D89-9C1E-5A2B3C4D5E6F}";
const SHELL32: &str = r"%SystemRoot%\system32\shell32.dll";
const SHELL32_ICON: &str = r"%SystemRoot%\system32\shell32.dll,-16769";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorerQuota {
    pub used: u64,
    pub total: u64,
    pub free: u64,
}

pub fn compute_explorer_quota(used_and_quotas: &[(u64, u64)]) -> Option<ExplorerQuota> {
    if used_and_quotas.is_empty() {
        return None;
    }
    let (mut used, mut total, mut free) = (0u64, 0u64, 0u64);
    for &(u, q) in used_and_quotas {
        if q == 0 || q < u {
            return None;
        }
        (used, total, free) = (
            used.saturating_add(u),
            total.saturating_add(q),
            free.saturating_add(q - u),
        );
    }
    Some(ExplorerQuota { used, total, free })
}

pub fn format_bytes_compact(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let (mut val, mut idx) = (bytes as f64, 0usize);
    while val >= 1024.0 && idx + 1 < UNITS.len() {
        val /= 1024.0;
        idx += 1;
    }
    let p = usize::from(idx > 0 && val < 10.0);
    format!("{val:.p$} {}", UNITS[idx])
}

pub fn format_explorer_quota_label(quota: Option<ExplorerQuota>) -> String {
    let Some(q) = quota else {
        return "OpenDrive".into();
    };
    let (u, t) = (format_bytes_compact(q.used), format_bytes_compact(q.total));
    format!("OpenDrive ({u} / {t})")
}

pub fn format_explorer_tile_comment(quota: Option<ExplorerQuota>) -> Option<String> {
    let q = quota?;
    let (f, t, u) = (
        format_bytes_compact(q.free),
        format_bytes_compact(q.total),
        format_bytes_compact(q.used),
    );
    Some(format!("{f} free of {t} ({u} used)"))
}

fn reg_add_key(k: &str, v: Option<&str>, t: &str, d: &str) -> Vec<String> {
    let mut args = vec!["add".into(), k.into()];
    match v {
        Some(n) => args.extend(["/v".into(), n.into()]),
        None => args.push("/ve".into()),
    }
    args.extend(["/t", t, "/d", d, "/f"].map(Into::into));
    args
}

fn reg_add(sub: &str, v: Option<&str>, t: &str, d: &str) -> Vec<String> {
    let k = if sub.is_empty() {
        CLSID_KEY.into()
    } else {
        format!(r"{CLSID_KEY}\{sub}")
    };
    reg_add_key(&k, v, t, d)
}

fn reg_delete(key: &str, val: Option<&str>) -> Vec<String> {
    let mut args = vec!["delete".into(), key.into()];
    if let Some(v) = val {
        args.extend(["/v".into(), v.into()]);
    }
    args.push("/f".into());
    args
}

/// Pure helper building `reg.exe` argument vectors for This PC Explorer namespace registration.
pub fn build_this_pc_reg_commands(
    enabled: bool,
    target: &Path,
    icon_ico: Option<&Path>,
    quota: Option<ExplorerQuota>,
) -> Vec<Vec<String>> {
    if !enabled {
        return [NAMESPACE_KEY, DESKTOP_NAMESPACE_KEY, CLSID_KEY]
            .into_iter()
            .map(|k| reg_delete(k, None))
            .collect();
    }
    let mut cmds = vec![reg_add("", None, "REG_SZ", "OpenDrive")];
    match format_explorer_tile_comment(quota) {
        Some(c) => cmds.extend([
            reg_add("", Some("TileInfo"), "REG_SZ", "prop:System.Comment"),
            reg_add("", Some("System.Comment"), "REG_SZ", &c),
        ]),
        None => cmds.extend([
            reg_delete(CLSID_KEY, Some("TileInfo")),
            reg_delete(CLSID_KEY, Some("System.Comment")),
        ]),
    }

    let (icon_type, icon_val) = icon_ico.map_or(("REG_EXPAND_SZ", SHELL32_ICON.into()), |ico| {
        ("REG_SZ", format!("{},0", ico.display()))
    });
    let target_str = target.display().to_string();
    let (ips, bag, fsf) = (
        "InProcServer32",
        r"Instance\InitPropertyBag",
        FILE_SYSTEM_FOLDER_CLSID,
    );
    for (s, v, k, d) in [
        ("DefaultIcon", None, icon_type, icon_val.as_str()),
        (ips, None, "REG_EXPAND_SZ", SHELL32),
        (ips, Some("ThreadingModel"), "REG_SZ", "Apartment"),
        ("Instance", Some("CLSID"), "REG_SZ", fsf),
        (bag, Some("Attributes"), "REG_DWORD", "0x11"),
        (bag, Some("TargetFolderPath"), "REG_SZ", &target_str),
        ("ShellFolder", Some("FolderValueFlags"), "REG_DWORD", "0x28"),
        ("ShellFolder", Some("Attributes"), "REG_DWORD", "0xf080004d"),
    ] {
        cmds.push(reg_add(s, v, k, d));
    }
    cmds.push(reg_add_key(NAMESPACE_KEY, None, "REG_SZ", "OpenDrive"));
    cmds
}

#[cfg(windows)]
fn run_reg_cmd(program: &str, args: &[String]) -> Result<()> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match crate::drive_mount::run_hidden(program, &refs) {
        Err(_) if args.first().is_some_and(|a| a == "delete") => Ok(()),
        res => res.map(|_| ()),
    }
}

#[cfg(windows)]
fn sync_this_pc_with(
    enabled: bool,
    target: &Path,
    icon_ico: Option<&Path>,
    quota: Option<ExplorerQuota>,
    mut run: impl FnMut(&[String]) -> Result<()>,
) -> Result<()> {
    if enabled {
        std::fs::create_dir_all(target)
            .map_err(|e| PlatformError::Other(format!("create {}: {e}", target.display())))?;
    }
    let args = build_this_pc_reg_commands(enabled, target, icon_ico, quota);
    for row in args {
        run(&row)?;
    }
    Ok(())
}

pub fn sync_this_pc_namespace(
    enabled: bool,
    target: &Path,
    icon_ico: Option<&Path>,
    quota: Option<ExplorerQuota>,
) -> Result<()> {
    #[cfg(windows)]
    {
        let r = |a: &[String]| run_reg_cmd("reg", a);
        if enabled {
            sync_this_pc_with(true, target, icon_ico, quota, r)?;
        } else {
            crate::mount_cleanup::remove_namespace_with_shell_notify(
                OPENDRIVE_NAMESPACE_CLSID,
                || sync_this_pc_with(false, target, icon_ico, quota, r),
            )?;
        }
        crate::notify_shell_mounts_changed();
    }
    #[cfg(not(windows))]
    {
        let _ = (icon_ico, quota);
        if enabled {
            std::fs::create_dir_all(target)
                .map_err(|e| PlatformError::Other(format!("create {}: {e}", target.display())))?;
        }
    }
    Ok(())
}

fn split_command_tokens(raw: &str) -> Result<Vec<String>> {
    let (mut tokens, mut current, mut active_quote, mut had_token) =
        (Vec::new(), String::new(), None::<char>, false);
    for ch in raw.chars() {
        match active_quote {
            Some(q) if ch == q => active_quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => {
                active_quote = Some(ch);
                had_token = true;
            }
            None if ch.is_whitespace() => {
                if had_token || !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                    had_token = false;
                }
            }
            None => {
                current.push(ch);
                had_token = true;
            }
        }
    }
    if active_quote.is_some() {
        return Err(PlatformError::Other(
            "unterminated quote in app command".into(),
        ));
    }
    if had_token || !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

pub fn build_launch_invocation(app_command: &str, target: &str) -> Result<(String, Vec<String>)> {
    let mut tokens = split_command_tokens(app_command)?;
    if tokens.is_empty() || tokens[0].trim().is_empty() {
        return Err(PlatformError::Other("empty app command".into()));
    }
    let program = tokens.remove(0);
    let mut substituted = false;
    let mut args = Vec::with_capacity(tokens.len() + 1);
    for arg in tokens {
        substituted |= arg.contains("{target}") || arg.contains("{url}");
        args.push(arg.replace("{target}", target).replace("{url}", target));
    }
    if !substituted {
        args.push(target.to_string());
    }
    Ok((program, args))
}

pub fn launch_with_app(app_command: Option<&str>, target: &str) -> Result<()> {
    let Some(raw) = app_command.map(str::trim).filter(|s| !s.is_empty()) else {
        return open_path(target);
    };
    let (program, args) = build_launch_invocation(raw, target)?;
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| PlatformError::Other(format!("failed to launch {program}: {e}")))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn sync_this_pc_with_and_run_reg_cmd_branches() {
        let dir = std::env::temp_dir().join(format!("od-eq-{}", std::process::id()));
        let t = dir.join("OpenDrive");
        let ok = |_: &[String]| Ok(());
        sync_this_pc_with(true, &t, None, None, ok).unwrap();
        sync_this_pc_with(false, &t, None, None, ok).unwrap();
        let err = |_: &[String]| Err(PlatformError::Other("e".into()));
        assert!(sync_this_pc_with(false, &t, None, None, err).is_err());
        let v = |s: &[&str]| s.iter().map(|x| (*x).into()).collect::<Vec<_>>();
        assert!(run_reg_cmd("cmd", &v(&["/C", "echo ok"])).is_ok());
        assert!(run_reg_cmd("cmd", &v(&["/C", "exit /b 1"])).is_err());
        assert!(run_reg_cmd("cmd", &v(&["delete", "/C", "exit /b 1"])).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
