//! Edge-case, UI-widget, and error-branch coverage tests for font discovery, import, and Settings font picker.

mod common_auth_helpers;

use common_auth_helpers::run_frame;
use eframe::egui;
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::font_import::{
    extract_ttf_url_from_css, import_google_font_family, import_local_font_file,
};
use opendrive_app::runtime::fonts::{
    discover_fonts_in_dirs, discover_system_fonts, validate_font_bytes,
};
use opendrive_core::Settings;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use uuid::Uuid;

fn bundled_ttf() -> Vec<u8> {
    let defs = egui::FontDefinitions::default();
    let arc = &defs.font_data.values().next().expect("font").font;
    arc.as_ref().as_ref().to_vec()
}

fn build_synthetic_otto_mac_name(name: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"OTTO");
    b.extend_from_slice(&1u16.to_be_bytes());
    b.extend_from_slice(&[0u8; 6]);
    b.extend_from_slice(b"name");
    b.extend_from_slice(&0u32.to_be_bytes());
    let name_off = 28u32;
    let name_len = 6 + 12 + name.len() as u32;
    b.extend_from_slice(&name_off.to_be_bytes());
    b.extend_from_slice(&name_len.to_be_bytes());
    b.extend_from_slice(&0u16.to_be_bytes());
    b.extend_from_slice(&1u16.to_be_bytes());
    b.extend_from_slice(&18u16.to_be_bytes());
    b.extend_from_slice(&1u16.to_be_bytes()); // Mac platform
    b.extend_from_slice(&[0u8; 4]);
    b.extend_from_slice(&1u16.to_be_bytes()); // nameID=1
    b.extend_from_slice(&(name.len() as u16).to_be_bytes());
    b.extend_from_slice(&0u16.to_be_bytes());
    b.extend_from_slice(name);
    b
}

#[test]
fn font_parser_and_import_error_branches_work() {
    assert!(validate_font_bytes(&[0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(validate_font_bytes(b"ttcf\x00\x01\x00\x00\x00\x00\x00\x01").is_err());

    let mut no_name = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    no_name.extend_from_slice(b"head");
    no_name.extend_from_slice(&[0u8; 12]);
    assert_eq!(validate_font_bytes(&no_name).unwrap(), None);

    let mac_font = build_synthetic_otto_mac_name(b"MacCustom");
    assert_eq!(
        validate_font_bytes(&mac_font).unwrap().as_deref(),
        Some("MacCustom")
    );

    assert!(extract_ttf_url_from_css("src: url(https://broken").is_none());
    let tmp = std::env::temp_dir().join(format!("od-fedge-{}", Uuid::new_v4()));
    let sub = tmp.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("bad_stem_.ttf"), &mac_font).unwrap();
    let list = discover_fonts_in_dirs(std::slice::from_ref(&tmp), None);
    assert!(list.iter().any(|f| f.family == "MacCustom"));
    let _ = discover_system_fonts(Some(&tmp));

    assert!(import_local_font_file(&tmp.join("missing.ttf"), &tmp).is_err());
    let file_as_dir = tmp.join("not_a_dir");
    std::fs::write(&file_as_dir, b"x").unwrap();
    assert!(import_local_font_file(&sub.join("bad_stem_.ttf"), &file_as_dir).is_err());

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let _srv = std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 1024];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let resp = if req.contains("family=NoTtf") {
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string()
            } else if req.contains("GET /bad.ttf") {
                "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            } else {
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\nsrc: url(http://{addr}/bad.ttf)", 24 + addr.to_string().len())
            };
            let _ = s.write_all(resp.as_bytes());
        }
    });
    let base = format!("http://{addr}/css2");
    assert!(import_google_font_family("NoTtf", &base, &tmp).is_err());
    assert!(import_google_font_family("BadFontFile", &base, &tmp).is_err());
    assert!(import_google_font_family("Dead", "http://127.0.0.1:1/css2", &tmp).is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

fn spawn_instant_err_server() -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 512];
            let _ = s.read(&mut buf);
            let _ = s.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    (format!("http://{addr}/css2"), handle)
}

fn click_ui_label(ctx: &egui::Context, app: &mut ControlCenterApp, label: &str) -> bool {
    let _ = run_frame(ctx, app, Vec::new());
    let out = run_frame(ctx, app, Vec::new());
    let mut found = None;
    for cs in &out.shapes {
        if let egui::Shape::Text(t) = &cs.shape {
            if t.galley.text() == label {
                found = Some(cs.shape.visual_bounding_rect().center());
            }
        }
    }
    let Some(pos) = found else {
        return false;
    };
    for pressed in [true, false] {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(960.0, 760.0),
            )),
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let mut o = ctx.run_ui(raw, |ui| app.render_root(ui));
        o.textures_delta.clear();
    }
    true
}

#[test]
fn settings_font_ui_widgets_and_action_fallbacks_work() {
    let tmp = std::env::temp_dir().join(format!("od-fui2-{}", Uuid::new_v4()));
    let cfg = tmp.join("cfg");
    let data = tmp.join("data");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let local_ttf = tmp.join("local_pick.ttf");
    std::fs::write(&local_ttf, bundled_ttf()).unwrap();
    let bad_ttf = tmp.join("bad_pick.ttf");
    std::fs::write(&bad_ttf, b"not-a-font").unwrap();

    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), engine, Some(cfg));
    let bridge = bridge.with_data_dir(data);

    let (err_base, _err_srv) = spawn_instant_err_server();
    let ep_none = AuthEndpoints {
        font_open_picker: Arc::new(|| None),
        google_fonts_css_base: err_base,
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), ep_none);
    let ctx = egui::Context::default();
    app.apply_action(UiAction::ToggleSettings(Some(true)));
    app.apply_action(UiAction::BrowseUiFont);
    app.apply_action(UiAction::SelectUiFont(String::new()));
    let _ = run_frame(&ctx, &mut app, Vec::new());

    // Local file path via background import worker
    app.font_draft.import_input = local_ttf.to_string_lossy().into_owned();
    app.apply_action(UiAction::ImportUiFont);
    for _ in 0..80 {
        let _ = run_frame(&ctx, &mut app, Vec::new());
        if !app.font_draft.importing {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!app.font_draft.importing);
    assert!(app.state.settings.ui_font_family.is_some());
    app.apply_action(UiAction::ToggleTheme);
    let _ = run_frame(&ctx, &mut app, Vec::new());
    let fkey = ctx.data(|d| d.get_temp::<String>(egui::Id::unique("od_applied_font_key")));
    assert_eq!(
        fkey.as_deref(),
        app.state.settings.ui_font_family.as_deref()
    );
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::Vec2::new(0.0, -900.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
        source: egui::MouseWheelSource::Unknown,
    };
    for _ in 0..3 {
        let mut o = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::Vec2::new(960.0, 760.0),
                )),
                events: vec![
                    egui::Event::PointerMoved(egui::Pos2::new(480.0, 380.0)),
                    wheel.clone(),
                ],
                ..Default::default()
            },
            |ui| app.render_root(ui),
        );
        o.textures_delta.clear();
    }
    let active = app.state.settings.ui_font_family.clone().unwrap();
    if click_ui_label(&ctx, &mut app, &active) {
        let _ = click_ui_label(&ctx, &mut app, "OpenDrive Default");
    }
    let _ = click_ui_label(&ctx, &mut app, "Browse");
    let _ = click_ui_label(&ctx, &mut app, "Reset Font");

    // Background worker failure path (instant HTTP 404 mock)
    app.font_draft.import_input = "WillFailRemote".into();
    app.apply_action(UiAction::ImportUiFont);
    for _ in 0..80 {
        let _ = run_frame(&ctx, &mut app, Vec::new());
        if !app.font_draft.importing {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!app.font_draft.importing);
    assert!(app.font_draft.feedback_is_err);
    let _ = run_frame(&ctx, &mut app, Vec::new());

    // Browse invalid font file surfaces error
    let ep_bad = AuthEndpoints {
        font_open_picker: Arc::new(move || Some(bad_ttf.clone())),
        ..Default::default()
    };
    let mut app2 = ControlCenterApp::with_auth_endpoints(bridge, ep_bad);
    app2.apply_action(UiAction::BrowseUiFont);
    assert!(app2.font_draft.feedback_is_err);
    let _ = std::fs::remove_dir_all(&tmp);
}
