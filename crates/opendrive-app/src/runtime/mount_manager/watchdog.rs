//! OS mount watchdog spawning, PID-ownership tracking, and Windows exit cleanup guards.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static WATCHDOG_STARTED: AtomicBool = AtomicBool::new(false);

fn configured_drive_letter() -> String {
    opendrive_platform::config_dir()
        .ok()
        .and_then(|d| opendrive_core::load_settings(&d).ok())
        .map(|s| s.virtual_drive_letter)
        .unwrap_or_else(|| opendrive_core::Settings::default().virtual_drive_letter)
}

fn mount_owner_pid_path() -> Option<PathBuf> {
    opendrive_platform::data_dir()
        .ok()
        .map(|d| d.join("mount-owner.pid"))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cleanup_os_mounts_with(
    mount_root: &Path,
    letter: &str,
    cfg_letter: &str,
    unregister_all: impl FnOnce(),
    unpin_pc: impl FnOnce(&Path),
    mut unmount: impl FnMut(&str, &Path),
    notify: impl FnOnce(),
) {
    unregister_all();
    unpin_pc(mount_root);
    unmount(letter, mount_root);
    if !cfg_letter.eq_ignore_ascii_case(letter) {
        unmount(cfg_letter, mount_root);
    }
    notify();
}

pub(super) fn unregister_all_os_roots() {
    #[cfg(windows)]
    opendrive_fs_windows::unregister_all_opendrive_roots();
}

/// Unmounts all OpenDrive Windows Cloud Files sync roots, `This PC` namespace, and `subst` drives.
pub fn cleanup_os_mounts(mount_root: &Path, letter: &str) {
    let cfg_letter = configured_drive_letter();
    cleanup_os_mounts_with(
        mount_root,
        letter,
        &cfg_letter,
        unregister_all_os_roots,
        |root| {
            let _ = opendrive_platform::sync_this_pc_namespace(false, root, None, None);
        },
        |l, root| {
            let _ = opendrive_platform::unmount_virtual_drive_for_target(l, root);
        },
        opendrive_platform::notify_shell_mounts_changed,
    );
}

pub(crate) fn is_foreign_owner_in(pid_path: Option<&Path>, self_pid: u32) -> bool {
    let Some(path) = pid_path else { return false };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .is_some_and(|owner_pid| {
            owner_pid != self_pid && opendrive_platform::is_process_alive(owner_pid)
        })
}

/// Returns `true` when another live OpenDrive process owns active OS mounts.
pub fn is_foreign_mount_owner_alive() -> bool {
    is_foreign_owner_in(mount_owner_pid_path().as_deref(), std::process::id())
}

fn cleanup_os_mounts_with_pid_file(
    pid_path: Option<&Path>,
    exited_pid: u32,
    cleanup: impl FnOnce(),
) {
    if is_foreign_owner_in(pid_path, exited_pid) {
        return;
    }
    let _ = pid_path.map(std::fs::remove_file);
    cleanup();
}

/// Cleans up OS mounts for an exited process unless another live OpenDrive instance claimed ownership.
pub fn cleanup_os_mounts_for_pid(exited_pid: u32, mount_root: &Path, letter: &str) {
    cleanup_os_mounts_with_pid_file(mount_owner_pid_path().as_deref(), exited_pid, || {
        cleanup_os_mounts(mount_root, letter);
    });
}

/// Installs Windows console Ctrl/Close/Shutdown handlers that remove OS mounts before exit.
pub fn install_process_mount_guards(root: &Path, letter: Option<&str>) {
    let guard_root = root.to_path_buf();
    let guard_letter = letter.map_or_else(configured_drive_letter, str::to_owned);
    opendrive_platform::install_console_cleanup_handler(move || {
        cleanup_os_mounts_for_pid(std::process::id(), &guard_root, &guard_letter);
    });
}

pub(crate) fn spawn_watchdog_with(
    pid_path: Option<&Path>,
    target: (&Path, Option<&str>),
    gate: (bool, Option<&Path>, &AtomicBool),
    cfg_letter: fn() -> String,
    spawn_bg: impl FnOnce(&Path, &[&str]) -> std::io::Result<()>,
) {
    let ((root, letter), (disabled, exe, started)) = (target, gate);
    if disabled {
        return;
    }
    if let Some(p) = pid_path {
        let _ = p.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(p, std::process::id().to_string());
    }
    if let Some(exe) =
        exe.filter(|e| e.parent().and_then(|p| p.file_name()) != Some("deps".as_ref()))
    {
        if !started.swap(true, Ordering::SeqCst) {
            let resolved = letter.map(str::to_owned).unwrap_or_else(cfg_letter);
            let (pid, root_s) = (std::process::id().to_string(), root.to_string_lossy());
            if let Err(err) = spawn_bg(exe, &["--mount-watchdog", &pid, &root_s, &resolved]) {
                started.store(false, Ordering::SeqCst);
                tracing::warn!(error = %err, "failed to spawn mount watchdog");
            }
        }
    }
}

/// Spawns a detached watchdog process that removes OS mounts if this process exits or is killed.
pub fn spawn_mount_watchdog(root: &Path, letter: Option<&str>) {
    install_process_mount_guards(root, letter);
    let exe = std::env::current_exe().ok();
    let disabled = std::env::var_os("OPENDRIVE_DISABLE_WATCHDOG").is_some();
    spawn_watchdog_with(
        mount_owner_pid_path().as_deref(),
        (root, letter),
        (disabled, exe.as_deref(), &WATCHDOG_STARTED),
        configured_drive_letter,
        |e, a| opendrive_platform::spawn_detached_background(e, a).map(|_| ()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_pid_guard_respects_live_owner_and_cleans_dead_owner() {
        let tmp = std::env::temp_dir().join(format!("od-pid-guard-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let pid_file = tmp.join("mount-owner.pid");
        std::fs::write(&pid_file, std::process::id().to_string()).unwrap();
        let mut ran = false;
        cleanup_os_mounts_with_pid_file(Some(&pid_file), u32::MAX, || ran = true);
        assert!(!ran && pid_file.is_file());
        std::fs::write(&pid_file, u32::MAX.to_string()).unwrap();
        cleanup_os_mounts_with_pid_file(Some(&pid_file), u32::MAX - 1, || ran = true);
        assert!(ran && !pid_file.exists());
        cleanup_os_mounts_with_pid_file(None, 1, || ran = false);
        assert!(!ran && configured_drive_letter().len() == 1 && mount_owner_pid_path().is_some());
        for (l, c, want) in [("o", "O", &["o"][..]), ("O", "P", &["O", "P"][..])] {
            let (mut calls, n) = (Vec::new(), || {});
            cleanup_os_mounts_with(&tmp, l, c, n, |_| {}, |x, _| calls.push(x.to_owned()), n);
            assert_eq!(calls, want);
        }
        let st = AtomicBool::new(false);
        let (pid2, exe, deps) = (tmp.join("s/p"), tmp.join("a"), tmp.join("deps/t"));
        for (p, l, dis, ex, ok) in [
            (Some(pid2.as_path()), None, true, Some(exe.as_path()), true),
            (Some(pid2.as_path()), None, false, None, true),
            (None, None, false, Some(deps.as_path()), true),
            (None, None, false, Some(exe.as_path()), false),
            (None, Some("Z"), false, Some(exe.as_path()), true),
            (None, None, false, Some(exe.as_path()), true),
        ] {
            spawn_watchdog_with(
                p,
                (&tmp, l),
                (dis, ex, &st),
                || "Q".into(),
                |_, _| ok.then_some(()).ok_or_else(|| std::io::Error::other("e")),
            );
        }
        assert!(st.load(Ordering::SeqCst) && pid2.is_file());
        super::super::MountManager::new_null().dehydrate_path(&tmp);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
