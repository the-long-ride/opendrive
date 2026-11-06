//! Unit tests for Neo-Cel Studio formatters and dual theme tokens.

use eframe::egui::{self, Theme};
use opendrive_app::design::formatters::{
    bandwidth_to_step, cache_to_step, compute_cache_ratio, default_folder_name, derive_monogram,
    format_account_storage, format_bandwidth, format_bytes, format_platform_label, join_mount_path,
    status_label, status_tone, step_to_bandwidth, step_to_cache, summarize_sync_health,
    validate_browser_oauth_draft, validate_service_account_draft, NewAccountDraft, StatusTone,
    BANDWIDTH_100M, BANDWIDTH_20M, BANDWIDTH_50M, BANDWIDTH_5M, GIB,
};
use opendrive_app::design::theme::{
    apply_studio_theme, palette, tone_badge_style, tone_colors, ThemeMode,
};
use opendrive_core::{Account, AccountStatus};
use uuid::Uuid;

fn sample_account(nickname: &str, email: &str, status: AccountStatus, read_only: bool) -> Account {
    Account {
        id: Uuid::new_v4(),
        nickname: nickname.into(),
        email: email.into(),
        credential_set_id: Uuid::nil(),
        folder_name: nickname.to_lowercase(),
        read_only,
        enabled: status != AccountStatus::Paused,
        status,
        storage_used: 0,
        storage_quota: 0,
        avatar_url: None,
    }
}

#[test]
fn formatters_steps_and_dual_theme_cover_all_variants() {
    assert_eq!(bandwidth_to_step(None), 0);
    assert_eq!(bandwidth_to_step(Some(0)), 0);
    assert_eq!(bandwidth_to_step(Some(BANDWIDTH_5M)), 1);
    assert_eq!(bandwidth_to_step(Some(BANDWIDTH_20M)), 2);
    assert_eq!(bandwidth_to_step(Some(BANDWIDTH_50M)), 3);
    assert_eq!(bandwidth_to_step(Some(BANDWIDTH_100M)), 4);
    assert_eq!(step_to_bandwidth(0), None);
    assert_eq!(step_to_bandwidth(3), Some(BANDWIDTH_50M));
    assert_eq!(step_to_bandwidth(99), Some(BANDWIDTH_100M));

    assert_eq!(cache_to_step(GIB), 0);
    assert_eq!(cache_to_step(5 * GIB), 1);
    assert_eq!(cache_to_step(10 * GIB), 2);
    assert_eq!(cache_to_step(25 * GIB), 3);
    assert_eq!(cache_to_step(50 * GIB), 4);
    assert_eq!(step_to_cache(0), GIB);
    assert_eq!(step_to_cache(3), 25 * GIB);
    assert_eq!(step_to_cache(99), 50 * GIB);

    let mut draft = NewAccountDraft::default();
    assert_eq!(
        validate_browser_oauth_draft(&draft),
        Some("Enter your Google OAuth Client ID.")
    );
    draft.client_id = "cid".into();
    assert_eq!(
        validate_browser_oauth_draft(&draft),
        Some("Enter your Google OAuth Client Secret.")
    );
    draft.client_secret = "sec".into();
    assert_eq!(validate_browser_oauth_draft(&draft), None);

    assert_eq!(
        validate_service_account_draft(&draft),
        Some("Paste Service Account JSON or enter the .json file path.")
    );
    draft.service_account_input = "{\"type\":\"service_account\"}".into();
    assert_eq!(validate_service_account_draft(&draft), None);

    assert_eq!(default_folder_name(" custom ", "Nick", "a@b.com"), "custom");
    assert_eq!(default_folder_name("  ", " Work ", "a@b.com"), "Work");
    assert_eq!(default_folder_name("", "", "studio@example.com"), "studio");
    assert_eq!(default_folder_name("", " ", "@example.com"), "drive");
    assert_eq!(default_folder_name("", "", ""), "drive");

    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1536), "1.5 KB");
    assert_eq!(format_bytes(12 * 1024 * 1024), "12 MB");
    assert_eq!(format_bandwidth(None), "Unlimited");
    assert_eq!(format_bandwidth(Some(BANDWIDTH_5M)), "5.0 MB/s");
    assert_eq!(format_account_storage(0, 0), None);
    assert_eq!(
        format_account_storage(512, 0).as_deref(),
        Some("512 B used")
    );
    assert_eq!(
        format_account_storage(5 * GIB, 10 * GIB).as_deref(),
        Some("5.0 GB / 10 GB (50%)")
    );

    assert_eq!(compute_cache_ratio(100, 0), 0);
    assert_eq!(compute_cache_ratio(85, 100), 85);
    assert_eq!(compute_cache_ratio(200, 100), 100);

    assert_eq!(derive_monogram("Work Drive", "w@x.com"), "WD");
    assert_eq!(derive_monogram("Studio", "s@x.com"), "ST");
    assert_eq!(derive_monogram("  ", "alex@co.com"), "AC");
    assert_eq!(derive_monogram("...", ""), "OD");

    assert_eq!(
        join_mount_path("C:\\Users\\OpenDrive\\", "\\work"),
        "C:\\Users\\OpenDrive\\work"
    );
    assert_eq!(
        join_mount_path("/home/u/OpenDrive/", "/work"),
        "/home/u/OpenDrive/work"
    );
    assert_eq!(join_mount_path("", "work"), "~/OpenDrive/work");

    for st in [
        AccountStatus::Connected,
        AccountStatus::Paused,
        AccountStatus::NeedsReauth,
        AccountStatus::Error,
    ] {
        assert!(!status_label(&st).is_empty());
        let _ = status_tone(&st);
    }

    assert_eq!(summarize_sync_health(&[]).tone, StatusTone::Standby);
    assert_eq!(
        summarize_sync_health(&[sample_account("A", "a@b", AccountStatus::Error, false)]).tone,
        StatusTone::Warn
    );
    assert_eq!(
        summarize_sync_health(&[sample_account("A", "a@b", AccountStatus::Connected, false)]).tone,
        StatusTone::Connected
    );
    assert_eq!(
        summarize_sync_health(&[sample_account("A", "a@b", AccountStatus::Paused, false)]).tone,
        StatusTone::Paused
    );

    assert_eq!(format_platform_label("macos"), "macOS");
    assert_eq!(format_platform_label("windows"), "Windows");
    assert_eq!(format_platform_label("linux"), "Linux");
    assert_eq!(format_platform_label("other"), "Desktop");

    assert_eq!(ThemeMode::ColorPop.toggle(), ThemeMode::DarkStudio);
    assert_eq!(ThemeMode::DarkStudio.toggle(), ThemeMode::ColorPop);
    let ctx = egui::Context::default();
    for (mode, egui_theme) in [
        (ThemeMode::ColorPop, Theme::Light),
        (ThemeMode::DarkStudio, Theme::Dark),
    ] {
        let pal = palette(mode);
        assert_ne!(pal.canvas_bg, pal.text_primary);
        assert_ne!(pal.dock_bg, pal.dock_fg);
        for tone in [
            StatusTone::Connected,
            StatusTone::Paused,
            StatusTone::Standby,
            StatusTone::Warn,
            StatusTone::Error,
        ] {
            let (bg, fg) = tone_colors(&pal, tone);
            assert_ne!(bg, fg);
            let (b_bg, b_fg, b_border) = tone_badge_style(&pal, tone);
            assert_eq!((b_bg, b_fg), (bg, fg));
            assert_ne!(b_bg, b_border);
        }
        apply_studio_theme(&ctx, mode);
        let vis = &ctx.style_of(egui_theme).visuals;
        assert_eq!(vis.panel_fill, pal.canvas_bg);
        assert_ne!(vis.selection.bg_fill, vis.selection.stroke.color);
    }
}
