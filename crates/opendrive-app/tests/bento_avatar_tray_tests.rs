//! Integration tests for Bento total storage, OpenDrive logo, account avatar caching/rendering, and tray lifecycle.

mod common_auth_helpers;

use common_auth_helpers::run_frame;
use eframe::egui;
use image::{ImageFormat, RgbaImage};
use opendrive_app::design::avatars::{
    avatar_file_path, cache_account_avatar, decode_avatar_color_image, load_cached_avatar_bytes,
};
use opendrive_app::design::formatters::{summarize_total_storage, GIB};
use opendrive_app::design::logo::{ensure_logo_ico, logo_icon_data, render_logo_rgba};
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::shell::tray::{apply_tray_event, route_tray_action, TrayUiEvent};
use opendrive_core::{Account, AccountStatus, Settings};
use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeDrive, FakeDriveServer};
use opendrive_platform::TrayAction;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn make_acct(used: u64, quota: u64) -> Account {
    Account {
        id: Uuid::new_v4(),
        nickname: "Demo".into(),
        email: "demo@example.com".into(),
        credential_set_id: Uuid::nil(),
        folder_name: "Demo".into(),
        read_only: false,
        enabled: true,
        status: AccountStatus::Connected,
        storage_used: used,
        storage_quota: quota,
        avatar_url: None,
    }
}

fn tiny_png_bytes() -> Vec<u8> {
    let img = RgbaImage::from_pixel(8, 8, image::Rgba([0xff, 0x3d, 0x7f, 0xff]));
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

#[test]
fn total_storage_summary_and_logo_rgba_work() {
    let empty = summarize_total_storage(&[]);
    assert!(!empty.has_exact_total && empty.badge_text == "0 B used" && empty.ratio == 0.0);

    let no_quota = summarize_total_storage(&[make_acct(2 * GIB, 0)]);
    assert!(!no_quota.has_exact_total && no_quota.badge_text == "2.0 GB used");

    let mixed = summarize_total_storage(&[make_acct(3 * GIB, 10 * GIB), make_acct(2 * GIB, 0)]);
    assert!(!mixed.has_exact_total && mixed.quota == 0 && mixed.badge_text == "5.0 GB used");

    let multi =
        summarize_total_storage(&[make_acct(3 * GIB, 10 * GIB), make_acct(2 * GIB, 10 * GIB)]);
    assert!(multi.has_exact_total && multi.used == 5 * GIB && multi.quota == 20 * GIB);
    assert!(multi.badge_text == "5.0 GB / 20 GB" && (multi.ratio - 0.25).abs() < 0.001);

    let rgba = render_logo_rgba(32);
    assert_eq!(rgba.len(), 32 * 32 * 4);
    let pixels = rgba.as_chunks::<4>().0;
    assert!(pixels.contains(&[0xff, 0x3d, 0x7f, 0xff]));
    assert!(pixels.contains(&[0x00, 0xd4, 0xff, 0xff]));
    let icon = logo_icon_data(64);
    assert_eq!((icon.width, icon.height), (64, 64));

    let wide = RgbaImage::from_pixel(40, 20, image::Rgba([0xff, 0x3d, 0x7f, 0xff]));
    let mut wide_png = Vec::new();
    wide.write_to(&mut Cursor::new(&mut wide_png), ImageFormat::Png)
        .unwrap();
    let contained = decode_avatar_color_image(&wide_png).unwrap();
    assert_eq!(contained.size, [72, 72]);
    assert_eq!(contained.pixels[0].a(), 0);
    assert_eq!(contained.pixels[36 * 72 + 36].a(), 255);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn avatar_download_cache_decoding_and_tray_close_lifecycle() {
    let tmp = std::env::temp_dir().join(format!("od-avatar-tray-{}", Uuid::new_v4()));
    let drive = Arc::new(FakeDrive::new());
    let png = tiny_png_bytes();
    drive.set_static_asset("/assets/user.png", png.clone());
    drive.set_static_asset("/assets/bad.png", b"not-an-image".to_vec());
    let (addr, handle) = FakeDriveServer::start(Arc::clone(&drive)).await;
    let client = DriveClient::with_base_urls(
        "tok".into(),
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );

    let acct = make_acct(1024, 4096);
    assert!(!cache_account_avatar(&client, &tmp, acct.id, None).await);
    assert!(
        !cache_account_avatar(
            &client,
            &tmp,
            acct.id,
            Some(&format!("http://{addr}/assets/bad.png"))
        )
        .await
    );
    assert!(
        cache_account_avatar(
            &client,
            &tmp,
            acct.id,
            Some(&format!("http://{addr}/assets/user.png"))
        )
        .await
    );
    let cached = load_cached_avatar_bytes(&tmp, acct.id).unwrap();
    assert_eq!(cached, png);
    assert!(avatar_file_path(&tmp, acct.id).exists());
    assert!(decode_avatar_color_image(b"corrupt").is_none());

    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), engine, None);
    let bridge = bridge.with_data_dir(tmp.clone());
    bridge
        .controller_handle()
        .lock()
        .unwrap()
        .accounts
        .add_account(acct);

    let queue = Arc::new(Mutex::new(VecDeque::new()));
    let mut app = ControlCenterApp::new(bridge).with_tray_events(Arc::clone(&queue), true);
    let ctx = egui::Context::default();
    let _ = run_frame(&ctx, &mut app, Vec::new());

    app.apply_action(UiAction::WindowRequestClose);
    assert!(app.hidden_to_tray);
    assert!(!app.quit_requested);

    queue.lock().unwrap().push_back(TrayUiEvent::ShowWindow);
    queue.lock().unwrap().push_back(TrayUiEvent::PauseAll);
    let _ = run_frame(&ctx, &mut app, Vec::new());
    assert!(!app.hidden_to_tray);

    for (action, expected) in [
        (TrayAction::Summary, None),
        (TrayAction::ShowWindow, Some(TrayUiEvent::ShowWindow)),
        (TrayAction::OpenFolder, Some(TrayUiEvent::OpenFolder)),
        (TrayAction::PauseAll, Some(TrayUiEvent::PauseAll)),
        (TrayAction::ResumeAll, Some(TrayUiEvent::ResumeAll)),
        (TrayAction::Quit, Some(TrayUiEvent::Quit)),
    ] {
        assert_eq!(route_tray_action(&action), expected);
    }

    let mut routed = Vec::new();
    for ev in [
        TrayUiEvent::OpenFolder,
        TrayUiEvent::ResumeAll,
        TrayUiEvent::Quit,
    ] {
        apply_tray_event(ev, &mut routed);
    }
    assert!(routed.contains(&UiAction::WindowForceQuit));
    app.apply_action(UiAction::WindowForceQuit);
    assert!(app.quit_requested);

    let ico = ensure_logo_ico(&tmp).unwrap();
    let ico_bytes = std::fs::read(&ico).unwrap();
    assert_eq!(&ico_bytes[..6], &[0, 0, 1, 0, 1, 0]);
    handle.abort();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn primary_and_dock_pill_button_text_color_never_matches_dark_background() {
    use eframe::egui::{Color32, Pos2, RawInput, Rect, Shape, Vec2};
    use opendrive_app::design::theme::{palette, ThemeMode};
    use opendrive_app::design::widgets::{pill_button, ButtonVariant, IconKind};

    let text_colors = |shape: &Shape| -> Option<Vec<Color32>> {
        match shape {
            Shape::Text(ts) => Some(
                ts.galley
                    .job
                    .sections
                    .iter()
                    .map(|s| s.format.color)
                    .collect(),
            ),
            _ => None,
        }
    };

    let ctx = egui::Context::default();
    for mode in [ThemeMode::ColorPop, ThemeMode::DarkStudio] {
        let pal = palette(mode);
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 120.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            let p = ButtonVariant::Primary;
            let d = ButtonVariant::TactilePill;
            let _ = pill_button(ui, &pal, Some(IconKind::Plus), "Connect", p, false, true);
            let _ = pill_button(
                ui,
                &pal,
                Some(IconKind::ThemeColor),
                "Light",
                d,
                false,
                true,
            );
        });
        out.textures_delta.clear();

        let text_lists: Vec<Vec<Color32>> = out
            .shapes
            .iter()
            .filter_map(|cs| text_colors(&cs.shape))
            .collect();
        assert_eq!(text_lists.len(), 2);
        assert!(text_lists[0].iter().all(|&c| c == Color32::WHITE));
        assert!(text_lists[0]
            .iter()
            .all(|&c| c != pal.accent_blue && c != pal.tactile_ink));
        assert!(text_lists[1].iter().all(|&c| c == pal.dock_fg));
        assert!(text_lists[1].iter().all(|&c| c != pal.dock_bg));
    }
}

#[test]
fn pastel_usage_colors_and_frameless_resize_directions_work() {
    use eframe::egui::{
        Event, Modifiers, PointerButton, Pos2, RawInput, Rect, ResizeDirection, Vec2,
    };
    use opendrive_app::design::theme::{palette, ThemeMode};
    use opendrive_app::design::widgets::usage_bar_color;
    use opendrive_app::desktop::resize::{
        detect_resize_direction, handle_frameless_resize, resize_cursor_icon,
    };

    let pal = palette(ThemeMode::DarkStudio);
    assert_eq!(usage_bar_color(&pal, 0.25), pal.pastel_green);
    assert_eq!(usage_bar_color(&pal, 0.749), pal.pastel_green);
    assert_eq!(usage_bar_color(&pal, 0.75), pal.pastel_orange);
    assert_eq!(usage_bar_color(&pal, 0.89), pal.pastel_orange);
    assert_eq!(usage_bar_color(&pal, 0.90), pal.pastel_red);
    assert_eq!(usage_bar_color(&pal, 1.0), pal.pastel_red);

    let r = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(600.0, 500.0));
    let cases = [
        (2., 2., Some(ResizeDirection::NorthWest)),
        (598., 2., Some(ResizeDirection::NorthEast)),
        (2., 498., Some(ResizeDirection::SouthWest)),
        (598., 498., Some(ResizeDirection::SouthEast)),
        (300., 2., Some(ResizeDirection::North)),
        (300., 498., Some(ResizeDirection::South)),
        (2., 250., Some(ResizeDirection::West)),
        (598., 250., Some(ResizeDirection::East)),
        (300., 250., None),
    ];
    for (x, y, expected) in cases {
        assert_eq!(detect_resize_direction(r, Pos2::new(x, y), 6.0), expected);
        if let Some(dir) = expected {
            let _ = resize_cursor_icon(dir);
        }
    }

    let ctx = egui::Context::default();
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 500.0))),
        events: vec![
            Event::PointerMoved(Pos2::new(2.0, 250.0)),
            Event::PointerButton {
                pos: Pos2::new(2.0, 250.0),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| handle_frameless_resize(ui.ctx()));
    out.textures_delta.clear();
}
