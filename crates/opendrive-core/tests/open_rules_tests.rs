//! Unit tests for media kind classification, open-with rule resolution, and stream capability.

use opendrive_core::{
    classify_media_kind, load_settings, normalize_rule_tokens, resolve_open_app, save_settings,
    supports_http_stream_arg, validate_open_with_rule, MediaKind, OpenWithRule, Settings,
};
use uuid::Uuid;

fn rule(pattern: &str, app_command: &str) -> OpenWithRule {
    OpenWithRule {
        pattern: pattern.into(),
        app_command: app_command.into(),
    }
}

#[test]
fn classifies_all_media_kinds_by_mime_and_extension() {
    assert_eq!(
        classify_media_kind("Photos", Some("application/vnd.google-apps.folder")),
        MediaKind::Folder
    );
    assert_eq!(
        classify_media_kind("Roadmap", Some("application/vnd.google-apps.document")),
        MediaKind::NativeDoc
    );
    assert_eq!(
        classify_media_kind("Budget", Some("application/vnd.google-apps.spreadsheet")),
        MediaKind::NativeDoc
    );
    assert_eq!(
        classify_media_kind("Deck", Some("application/vnd.google-apps.presentation")),
        MediaKind::NativeDoc
    );
    assert_eq!(classify_media_kind("pic.PNG", None), MediaKind::Image);
    assert_eq!(
        classify_media_kind("blob", Some("image/webp")),
        MediaKind::Image
    );
    assert_eq!(classify_media_kind("clip.mkv", None), MediaKind::Video);
    assert_eq!(
        classify_media_kind("stream", Some("video/mp4")),
        MediaKind::Video
    );
    assert_eq!(classify_media_kind("manual.PDF", None), MediaKind::Pdf);
    assert_eq!(
        classify_media_kind("doc", Some("application/pdf")),
        MediaKind::Pdf
    );
    assert_eq!(classify_media_kind("track.flac", None), MediaKind::Audio);
    assert_eq!(
        classify_media_kind("voice", Some("audio/mpeg")),
        MediaKind::Audio
    );
    assert_eq!(classify_media_kind("backup.7z", None), MediaKind::Archive);
    assert_eq!(classify_media_kind("lib.rs", None), MediaKind::CodeText);
    assert_eq!(
        classify_media_kind("notes", Some("text/plain")),
        MediaKind::CodeText
    );
    assert_eq!(classify_media_kind("README", None), MediaKind::Other);
    assert_eq!(
        classify_media_kind("image.bin", Some("application/octet-stream")),
        MediaKind::Other
    );
}

#[test]
fn resolves_open_app_across_extensions_tags_wildcards_and_skips_empty() {
    let rules = vec![
        rule(".mp4", "   "),
        rule(".MP4, mkv  mov", "mpv --fs"),
        rule("@video", "vlc"),
        rule("@image", "nomacs"),
        rule("@pdf", "sumatrapdf"),
        rule("@audio", "foobar2000"),
        rule("@code", "code"),
        rule("@archive", "7zfm"),
        rule("*", "notepad"),
    ];

    assert_eq!(
        resolve_open_app(&rules, "movie.mp4", None),
        Some("mpv --fs")
    );
    assert_eq!(resolve_open_app(&rules, "clip.MKV", None), Some("mpv --fs"));
    assert_eq!(resolve_open_app(&rules, "trailer.webm", None), Some("vlc"));
    assert_eq!(resolve_open_app(&rules, "photo.avif", None), Some("nomacs"));
    assert_eq!(
        resolve_open_app(&rules, "spec.pdf", None),
        Some("sumatrapdf")
    );
    assert_eq!(
        resolve_open_app(&rules, "song.opus", None),
        Some("foobar2000")
    );
    assert_eq!(resolve_open_app(&rules, "config.toml", None), Some("code"));
    assert_eq!(resolve_open_app(&rules, "bundle.tar", None), Some("7zfm"));
    assert_eq!(resolve_open_app(&rules, "blob.dat", None), Some("notepad"));

    // Exact extension matching must not match substrings.
    let ext_only = vec![rule("mp4", "vlc")];
    assert_eq!(resolve_open_app(&ext_only, "clip.mp40", None), None);
    assert_eq!(
        resolve_open_app(
            &ext_only,
            "folder.mp4",
            Some("application/vnd.google-apps.folder")
        ),
        None
    );
}

#[test]
fn detects_http_stream_capable_players_case_insensitively() {
    for cmd in [
        "vlc",
        "MPV --profile=fast",
        r#""C:\Program Files\VideoLAN\VLC\vlc.exe" --one-instance"#,
        "/Applications/IINA.app/Contents/MacOS/iina",
        "/usr/bin/ffplay",
        r"C:\Tools\mpc-hc64.exe",
        "mpc-be.exe",
        "PotPlayerMini64.exe",
        "smplayer",
        "celluloid",
        "totem",
        "QuickTime",
    ] {
        assert!(
            supports_http_stream_arg(Some(cmd)),
            "expected true for {cmd}"
        );
    }

    for cmd in [
        "",
        "   ",
        "code --wait",
        r#""C:\Windows\notepad.exe""#,
        "sumatrapdf",
    ] {
        assert!(
            !supports_http_stream_arg(Some(cmd)),
            "expected false for {cmd}"
        );
    }
    assert!(!supports_http_stream_arg(None));
}

#[test]
fn settings_persists_open_with_rules_and_defaults_empty() {
    let defaults = Settings::default();
    assert!(defaults.open_with_rules.is_empty() && defaults.explorer_folder_cache);

    let dir = std::env::temp_dir().join(format!("opendrive-open-rules-{}", Uuid::new_v4()));
    let settings = Settings {
        open_with_rules: vec![rule("@video", "mpv"), rule("pdf", "okular")],
        explorer_folder_cache: false,
        ..Settings::default()
    };
    save_settings(&dir, &settings).unwrap();

    let loaded = load_settings(&dir).unwrap();
    assert_eq!(loaded.open_with_rules, settings.open_with_rules);
    assert!(!loaded.explorer_folder_cache);

    // Backwards-compat JSON without open_with_rules/explorer_folder_cache uses defaults.
    let legacy = r#"{
        "root_path": "/r",
        "cache_dir": "/c",
        "cache_max_size": 1024,
        "bandwidth_limit": null,
        "autostart": false,
        "check_updates": false,
        "docs_mode": "Link"
    }"#;
    std::fs::write(opendrive_core::settings_path(&dir), legacy).unwrap();
    let compat = load_settings(&dir).unwrap();
    assert!(compat.open_with_rules.is_empty() && compat.explorer_folder_cache);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn validate_open_with_rule_normalizes_and_blocks_invalid_or_duplicate_extensions() {
    assert_eq!(
        normalize_rule_tokens(".MP4, mkv  *.MP4").unwrap(),
        vec!["mp4", "mkv"]
    );
    let first = validate_open_with_rule(".MP4, mkv  .mp4", "  vlc --fs ", &[], None).unwrap();
    assert_eq!(
        (first.pattern.as_str(), first.app_command.as_str()),
        ("mp4, mkv", "vlc --fs")
    );
    for (pat, cmd) in [
        ("", "vlc"),
        (".", "vlc"),
        ("*.", "vlc"),
        ("@unknown", "vlc"),
        ("foo.bar", "vlc"),
        ("mp*4", "vlc"),
        ("mp4", ""),
        ("mp4", "\"unclosed"),
    ] {
        assert!(
            validate_open_with_rule(pat, cmd, &[], None).is_err(),
            "{pat} -> {cmd}"
        );
    }
    let existing = vec![first, rule("@pdf", "sumatrapdf")];
    assert!(validate_open_with_rule(".MKV, avi", "mpv", &existing, None).is_err());
    assert!(validate_open_with_rule("pdf", "foxit", &existing, None).is_err());
    assert!(validate_open_with_rule("@pdf", "foxit", &existing, None).is_err());
    assert!(validate_open_with_rule("*", "notepad", &existing, None).is_err());
    assert!(validate_open_with_rule("mp4", "vlc", &[rule("*", "notepad")], None).is_err());
    assert!(validate_open_with_rule("*", "notepad", &[], None).is_ok());
    for (pat, cmd) in [("html", "chrome"), ("tiff", "gimp"), ("wmv", "mpv")] {
        assert!(validate_open_with_rule(pat, cmd, &existing, None).is_ok());
    }
    let ext_conflicts = vec![
        rule("@code", "code"),
        rule("@image", "irfan"),
        rule("@video", "vlc"),
    ];
    for (pat, cmd) in [("html", "chrome"), ("tiff", "gimp"), ("wmv", "mpv")] {
        assert!(
            validate_open_with_rule(pat, cmd, &ext_conflicts, None).is_err(),
            "{pat} should conflict with media group"
        );
    }
    let edited = validate_open_with_rule("mp4, webm", "mpv", &existing, Some(0)).unwrap();
    assert_eq!(edited.pattern, "mp4, webm");
}
