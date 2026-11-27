//! Unit tests for Neo-Cel Studio formatters and dual theme tokens.

use eframe::egui::{self, Pos2, RawInput, Rect, Shape, Theme, Vec2};
use opendrive_app::design::formatters::{
    bandwidth_to_step, cache_to_step, compute_cache_ratio, default_folder_name, derive_monogram,
    format_account_storage, format_bandwidth, format_bytes, format_modified_date,
    format_platform_label, join_mount_path, status_label, status_tone, step_to_bandwidth,
    step_to_cache, summarize_sync_health, validate_browser_oauth_draft,
    validate_service_account_draft, NewAccountDraft, StatusTone, BANDWIDTH_100M, BANDWIDTH_20M,
    BANDWIDTH_50M, BANDWIDTH_5M, GIB,
};
use opendrive_app::design::theme::{
    apply_studio_theme, palette, tone_badge_style, tone_colors, ThemeMode,
};
use opendrive_app::desktop::explorer::AccountExplorerSession;
use opendrive_app::desktop::{ControlCenterApp, ExplorerViewMode};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::explorer_ops::ExplorerEntry;
use opendrive_core::{Account, AccountStatus, MediaKind, Settings};
use opendrive_store::NodeState;
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

    assert_eq!(format_modified_date(0), "\u{2014}");
    assert_eq!(format_modified_date(-1000), "\u{2014}");
    assert_eq!(format_modified_date(1_767_225_600_000), "2026-01-01 00:00");
    assert_eq!(format_modified_date(1_709_213_400_000), "2024-02-29 13:30");
}

#[test]
fn explorer_list_and_grid_columns_align_and_render_unicode() {
    let mk_entry = |id, name: &str, kind, is_folder, size, modified_ms| ExplorerEntry {
        id,
        name: name.into(),
        media_kind: kind,
        is_folder,
        is_native_doc: false,
        mime: None,
        size,
        modified_ms,
        state: NodeState::Cached,
        sync_status: opendrive_sync::ExplorerSyncStatus::FullLocal,
        pinned: false,
        drive_id: format!("d-{id}"),
    };
    let (bridge, _rx) = Bridge::new(Settings::default());
    let acct_id = bridge
        .add_account("Work".into(), "w@x.com".into(), "Work".into(), false)
        .unwrap()
        .accounts[0]
        .id;
    let mut app = ControlCenterApp::new(bridge);
    app.explorer.open = true;
    app.explorer.active_account_id = Some(acct_id);
    app.explorer.sessions.insert(
        acct_id,
        AccountExplorerSession {
            breadcrumbs: vec![(1, "My Drive".into())],
            entries: vec![
                mk_entry(2, "Shared with me", MediaKind::Folder, true, 0, 0),
                mk_entry(
                    3,
                    "\u{8292}\u{679c}\u{67ef}\u{67ef} - Google Drive",
                    MediaKind::Other,
                    false,
                    74,
                    1_767_225_600_000,
                ),
            ],
            loaded: true,
            ..Default::default()
        },
    );
    let ctx = egui::Context::default();
    apply_studio_theme(&ctx, ThemeMode::DarkStudio);

    let render = |a: &mut ControlCenterApp| {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0))),
            ..Default::default()
        };
        ctx.run_ui(raw.clone(), |ui| a.render_root(ui))
            .textures_delta
            .clear();
        let mut out = ctx.run_ui(raw, |ui| a.render_root(ui));
        out.textures_delta.clear();
        out.shapes
    };
    fn walk<'a>(shapes: &'a [egui::epaint::ClippedShape], f: &mut impl FnMut(&'a Shape)) {
        shapes.iter().for_each(|cs| match &cs.shape {
            Shape::Vec(v) => v.iter().for_each(|s| match s {
                Shape::Vec(inner) => inner.iter().for_each(&mut *f),
                _ => f(s),
            }),
            s => f(s),
        });
    }

    let (mut rows, mut texts) = (Vec::new(), std::collections::HashMap::new());
    walk(&render(&mut app), &mut |s| match s {
        Shape::Rect(rs) if (rs.rect.height() - 36.0).abs() < 0.5 && rs.rect.left() > 280.0 => {
            rows.push(rs.rect);
        }
        Shape::Text(ts) => {
            let r = ts.galley.rect.translate(ts.pos.to_vec2());
            texts.insert(ts.galley.text().to_owned(), r);
        }
        _ => {}
    });
    assert_eq!(rows.len(), 2);
    assert!((rows[0].left() - rows[1].left()).abs() < 0.5);
    assert!((rows[0].width() - rows[1].width()).abs() < 0.5 && rows[0].width() > 500.0);

    let hdrs: Vec<Rect> = ["Name", "Sync State", "Modified", "Size"]
        .into_iter()
        .map(|k| texts[k])
        .collect();
    for w in hdrs.windows(2) {
        assert!(
            (w[0].center().y - w[1].center().y).abs() < 1.0 && w[1].left() - w[0].right() >= 12.0
        );
    }
    let (fn_r, mod_r, sz_r) = (
        texts["\u{8292}\u{679c}\u{67ef}\u{67ef} - Google Drive"],
        texts["2026-01-01 00:00"],
        texts["74 B"],
    );
    assert!(
        (fn_r.center().y - mod_r.center().y).abs() < 1.5
            && (mod_r.center().y - sz_r.center().y).abs() < 1.5
            && sz_r.left() - mod_r.right() >= 12.0
    );

    app.explorer.view_mode = ExplorerViewMode::Grid;
    let mut cards = Vec::new();
    walk(&render(&mut app), &mut |s| {
        if let Shape::Rect(rs) = s {
            if (rs.rect.height() - 106.0).abs() < 0.5 && rs.rect.left() > 280.0 {
                cards.push(rs.rect);
            }
        }
    });
    assert_eq!(cards.len(), 2);
    assert!((cards[0].width() - cards[1].width()).abs() < 0.5);
}
