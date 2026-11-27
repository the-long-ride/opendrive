//! Unit tests for Explorer quota formatting, This PC namespace reg commands, drive mount errors, and app launcher.

use opendrive_platform::{
    build_launch_invocation, build_this_pc_reg_commands, compute_explorer_quota,
    drive_icon_reg_command, drive_label_reg_command, drive_mount_command, drive_unmount_command,
    format_explorer_quota_label, format_explorer_tile_comment, install_console_cleanup_handler,
    is_virtual_drive_supported, launch_with_app, mount_virtual_drive, normalize_drive_letter,
    open_path, parse_subst_target, pick_available_drive_letter, spawn_detached_background,
    sync_this_pc_namespace, unmount_virtual_drive, unmount_virtual_drive_for_target, ExplorerQuota,
};
use std::path::{Path, PathBuf};

#[test]
fn compute_and_format_explorer_quota_all_units() {
    assert_eq!(compute_explorer_quota(&[]), None);
    assert_eq!(compute_explorer_quota(&[(100, 0)]), None);
    assert_eq!(compute_explorer_quota(&[(200, 100)]), None);
    assert_eq!(compute_explorer_quota(&[(10, 100), (50, 0)]), None);

    let kib = 1024u64;
    let mib = 1024 * kib;
    let gib = 1024 * mib;
    let tib = 1024 * gib;

    let multi = compute_explorer_quota(&[(4 * gib, 15 * gib), (gib, 5 * gib)]).unwrap();
    assert_eq!(
        multi,
        ExplorerQuota {
            used: 5 * gib,
            total: 20 * gib,
            free: 15 * gib,
        }
    );
    assert_eq!(format_explorer_quota_label(None), "OpenDrive");
    assert_eq!(format_explorer_tile_comment(None), None);
    assert_eq!(
        format_explorer_quota_label(Some(multi)),
        "OpenDrive (5.0 GB / 20 GB)"
    );
    assert_eq!(
        format_explorer_tile_comment(Some(multi)).as_deref(),
        Some("15 GB free of 20 GB (5.0 GB used)")
    );

    let cases = [
        ((0, 512), "OpenDrive (0 B / 512 B)"),
        ((5 * kib, 20 * kib), "OpenDrive (5.0 KB / 20 KB)"),
        ((3 * mib, 12 * mib), "OpenDrive (3.0 MB / 12 MB)"),
        ((2 * tib, 12 * tib), "OpenDrive (2.0 TB / 12 TB)"),
    ];
    for ((used, total), expected) in cases {
        let q = compute_explorer_quota(&[(used, total)]).unwrap();
        assert_eq!(format_explorer_quota_label(Some(q)), expected);
    }
}

#[test]
fn this_pc_reg_commands_cover_enabled_disabled_icon_and_quota_variants() {
    let target = Path::new(r"C:\Users\Alice\OpenDrive");
    let ico = Path::new(r"C:\Users\Alice\AppData\opendrive.ico");
    let quota = Some(ExplorerQuota {
        used: 1024,
        total: 4096,
        free: 3072,
    });

    let with_quota_ico = build_this_pc_reg_commands(true, target, Some(ico), quota);
    assert!(with_quota_ico
        .iter()
        .any(|c| c.contains(&"TileInfo".into()) && c.contains(&"prop:System.Comment".into())));
    assert!(with_quota_ico
        .iter()
        .any(|c| c.contains(&"System.Comment".into())));
    assert!(with_quota_ico
        .iter()
        .any(|c| c.contains(&"{0E5AAE11-A475-4c5b-AB00-C66DE400274E}".into())));
    assert!(with_quota_ico
        .iter()
        .any(|c| c.contains(&"FolderValueFlags".into()) && c.contains(&"0x28".into())));
    assert!(with_quota_ico
        .iter()
        .any(|c| c.contains(&r"C:\Users\Alice\OpenDrive".into())));
    assert!(with_quota_ico
        .iter()
        .any(|c| c.contains(&r"C:\Users\Alice\AppData\opendrive.ico,0".into())));
    assert!(with_quota_ico
        .last()
        .unwrap()
        .iter()
        .any(|s| s.contains(r"MyComputer\NameSpace")));

    let with_quota_default_ico = build_this_pc_reg_commands(true, target, None, quota);
    assert!(with_quota_default_ico
        .iter()
        .any(|c| c.contains(&"REG_EXPAND_SZ".into())
            && c.iter().any(|a| a.contains("shell32.dll,-16769"))));

    let without_quota_ico = build_this_pc_reg_commands(true, target, Some(ico), None);
    assert!(without_quota_ico
        .iter()
        .any(|c| c[0] == "delete" && c.contains(&"System.Comment".into())));
    assert!(without_quota_ico
        .iter()
        .any(|c| c.contains(&r"C:\Users\Alice\AppData\opendrive.ico,0".into())));

    let without_quota_default = build_this_pc_reg_commands(true, target, None, None);
    assert!(!without_quota_default
        .iter()
        .any(|c| c.contains(&"prop:System.Comment".into())));

    let disabled = build_this_pc_reg_commands(false, target, None, None);
    assert_eq!(disabled.len(), 3);
    assert_eq!(disabled[0][0], "delete");
    assert!(disabled[0][1].contains(r"MyComputer\NameSpace"));
    assert!(disabled[1][1].contains(r"Desktop\NameSpace"));
    assert!(disabled[2][1].contains(r"Software\Classes\CLSID"));
}

#[test]
fn spawn_detached_background_and_console_hook_succeed() {
    install_console_cleanup_handler(|| {});
    #[cfg(windows)]
    let (prog, args) = (Path::new("cmd"), ["/C", "exit", "0"].as_slice());
    #[cfg(not(windows))]
    let (prog, args) = (Path::new("sh"), ["-c", "exit 0"].as_slice());
    let mut child = spawn_detached_background(prog, args)
        .expect("detached background spawn must succeed without ERROR_INVALID_PARAMETER");
    assert!(child.wait().unwrap().success());
}

#[test]
fn build_launch_invocation_and_launcher_errors() {
    assert_eq!(
        build_launch_invocation(
            r#""C:\Program Files\VideoLAN\VLC\vlc.exe" --one-instance"#,
            "http://127.0.0.1:9000/v.mp4"
        )
        .unwrap(),
        (
            r"C:\Program Files\VideoLAN\VLC\vlc.exe".into(),
            vec![
                "--one-instance".into(),
                "http://127.0.0.1:9000/v.mp4".into()
            ]
        )
    );
    assert_eq!(
        build_launch_invocation("mpv --title={url} '{target}'", "https://ex/a.mkv").unwrap(),
        (
            "mpv".into(),
            vec!["--title=https://ex/a.mkv".into(), "https://ex/a.mkv".into()]
        )
    );
    assert_eq!(
        build_launch_invocation("ffplay   {target}", "v.mp4").unwrap(),
        ("ffplay".into(), vec!["v.mp4".into()])
    );

    for bad in ["", "   ", "\"unterminated", "'unterminated", "\"\""] {
        assert!(build_launch_invocation(bad, "a").is_err());
    }

    assert!(launch_with_app(Some("opendrive-nonexistent-player-xyz"), "file.mp4").is_err());
    assert!(launch_with_app(Some("\"unterminated"), "file.mp4").is_err());
    assert!(launch_with_app(Some("\"\""), "file.mp4").is_err());
    assert!(open_path("").is_err());
    assert!(open_path("   ").is_err());
    assert!(launch_with_app(None, "").is_err());
    assert!(launch_with_app(Some("   "), "   ").is_err());
}

#[test]
fn drive_mount_and_namespace_error_paths() {
    let dir = std::env::temp_dir().join(format!("od-plat-err-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, b"x").unwrap();

    assert!(mount_virtual_drive("C", &dir).is_err());
    assert!(mount_virtual_drive("", &dir).is_err());
    assert!(mount_virtual_drive("Y", &blocker.join("child")).is_err());
    assert!(unmount_virtual_drive("C").is_err());
    assert!(unmount_virtual_drive("").is_err());
    assert!(unmount_virtual_drive_for_target("C", &dir).is_err());
    assert!(sync_this_pc_namespace(true, &blocker.join("child"), None, None).is_err());

    let p1 = Path::new(r"C:\Users\Alice\OpenDrive");
    assert_eq!(
        pick_available_drive_letter("O", "", p1, |_| false).unwrap(),
        'O'
    );
    assert_eq!(
        pick_available_drive_letter("O", &format!("O:\\: => {}\n", p1.display()), p1, |_| true)
            .unwrap(),
        'O'
    );
    assert_eq!(
        pick_available_drive_letter("O", "O:\\: => C:\\Other\n", p1, |c| c == 'P').unwrap(),
        'Q'
    );
    assert_eq!(
        pick_available_drive_letter(
            "O",
            &format!("O:\\: => C:\\Other\nP:\\: => {}\n", p1.display()),
            p1,
            |_| true
        )
        .unwrap(),
        'P'
    );
    assert!(pick_available_drive_letter("O", "", p1, |_| true).is_err());
    assert!(pick_available_drive_letter("C", "", p1, |_| false).is_err());
    assert!(drive_label_reg_command('O', None)
        .1
        .contains(&"OpenDrive".into()));

    assert_eq!(normalize_drive_letter("o").unwrap(), 'O');
    assert_eq!(normalize_drive_letter("  z:\\ ").unwrap(), 'Z');
    assert_eq!(normalize_drive_letter("D:/").unwrap(), 'D');
    for bad in ["", "CD", "1:", "A:", "b", "C:\\"] {
        assert!(normalize_drive_letter(bad).is_err());
    }
    assert_eq!(is_virtual_drive_supported(), cfg!(windows));
    let t = Path::new(r"C:\Users\Alice\OpenDrive");
    assert_eq!(
        drive_mount_command('O', t).1,
        vec!["O:", r"C:\Users\Alice\OpenDrive"]
    );
    assert_eq!(drive_unmount_command('O').1, vec!["O:", "/D"]);
    let stdout = "D:\\: => C:\\Work\no:\\: =>   C:\\Users\\Alice\\OpenDrive  \nE:\\: =>   \n";
    assert_eq!(
        parse_subst_target(stdout, 'O'),
        Some(PathBuf::from(r"C:\Users\Alice\OpenDrive"))
    );
    assert_eq!(
        parse_subst_target(stdout, 'd'),
        Some(PathBuf::from(r"C:\Work"))
    );
    assert_eq!(parse_subst_target(stdout, 'E'), None);
    let q = ExplorerQuota {
        used: 2 << 30,
        total: 15 << 30,
        free: 13 << 30,
    };
    assert!(drive_label_reg_command('O', Some(q))
        .1
        .contains(&"OpenDrive (2.0 GB / 15 GB)".into()));
    assert!(drive_icon_reg_command('O', Path::new(r"C:\OD\a.ico"))
        .1
        .contains(&r"C:\OD\a.ico,0".into()));

    let _ = std::fs::remove_dir_all(&dir);
}
