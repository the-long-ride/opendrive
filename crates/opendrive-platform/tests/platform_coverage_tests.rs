//! Coverage tests for `opendrive-platform` top-level APIs and error formatting.

use opendrive_platform::{
    autostart_command, is_process_alive, notify, notify_shell_mounts_changed, open_drive_file,
    set_autostart, spawn_detached_background, wait_for_process_exit, PlatformError, TrayAction,
};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn error_display_tray_actions_and_autostart_on_off() {
    let err = PlatformError::Other("boom".into());
    assert_eq!(err.to_string(), "boom");
    assert!(format!("{err:?}").contains("boom"));

    let actions = [
        TrayAction::Summary,
        TrayAction::ShowWindow,
        TrayAction::OpenFolder,
        TrayAction::PauseAll,
        TrayAction::ResumeAll,
        TrayAction::Quit,
    ];
    assert_eq!(actions[0], actions[0].clone());
    assert_ne!(actions[0], actions[5]);
    assert!(format!("{actions:?}").contains("ShowWindow"));

    let exe = Path::new(r"C:\OpenDrive\opendrive-app.exe");
    let on = autostart_command(exe, true);
    let off = autostart_command(exe, false);
    assert!(on.contains("opendrive-app.exe"));
    assert_ne!(on, off);

    set_autostart(true).unwrap();
    set_autostart(false).unwrap();
    notify("OpenDrive", "Background check").unwrap();
}

#[test]
fn open_drive_file_rejects_empty_and_whitespace() {
    for bad in ["", "   ", "\t\n"] {
        let err = open_drive_file(bad).unwrap_err();
        assert!(err.to_string().contains("empty file id"));
    }
}

fn wait_for_exit_bounded(pid: u32) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        wait_for_process_exit(pid);
        let _ = tx.send(());
    });
    assert!(
        rx.recv_timeout(Duration::from_secs(5)).is_ok(),
        "wait_for_process_exit({pid}) timed out"
    );
}

#[test]
fn acquire_instance_lock_and_wait_for_process_exit() {
    notify_shell_mounts_changed();
    assert!(is_process_alive(std::process::id()));
    assert!(
        !is_process_alive(0) && !is_process_alive(i32::MAX as u32) && !is_process_alive(u32::MAX)
    );
    wait_for_exit_bounded(0);
    wait_for_exit_bounded(u32::MAX);

    let tmp = std::env::temp_dir().join(format!("od-plat-lock-{}", std::process::id()));
    let lock_path = tmp.join("opendrive.lock");
    assert!(opendrive_platform::acquire_instance_lock(&lock_path).is_ok());
    assert!(opendrive_platform::acquire_instance_lock(&lock_path).is_err());

    #[cfg(windows)]
    let (prog, args) = (Path::new("cmd"), ["/C", "exit", "0"].as_slice());
    #[cfg(not(windows))]
    let (prog, args) = (Path::new("sh"), ["-c", "exit 0"].as_slice());
    let mut child = spawn_detached_background(prog, args).unwrap();
    wait_for_exit_bounded(child.id());
    assert!(child.wait().unwrap().success());
    #[cfg(not(windows))]
    {
        let mut slow = spawn_detached_background(Path::new("sh"), &["-c", "sleep 0.06"]).unwrap();
        wait_for_exit_bounded(slow.id());
        assert!(slow.wait().unwrap().success());
    }
    let _ = std::fs::remove_dir_all(&tmp);

    assert!(opendrive_platform::normalize_drive_letter("A").is_err());
    assert!(opendrive_platform::normalize_drive_letter("12").is_err());
    assert_eq!(
        opendrive_platform::normalize_drive_letter(" z:\\ ").unwrap(),
        'Z'
    );
    assert_eq!(
        opendrive_platform::parse_subst_target("  z:\\: => C:\\Temp\\Mount \r\n", 'Z').as_deref(),
        Some(Path::new(r"C:\Temp\Mount"))
    );
    assert!(opendrive_platform::parse_subst_target("Z:\\: =>   \n", 'Z').is_none());
    assert!(opendrive_platform::mount_virtual_drive("C", Path::new(".")).is_err());
    assert!(opendrive_platform::unmount_virtual_drive("C").is_err());
}
