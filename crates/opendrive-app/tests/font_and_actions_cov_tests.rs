//! Coverage tests for font import URL fallbacks and font UI/actions edge paths.

mod common_auth_helpers;

use common_auth_helpers::run_frame;
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::desktop::modals::font_ui::{render_font_section, FontPickerDraft};
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::font_import::{
    import_font_from_url_with_policy, import_google_font_family, spawn_font_import,
};
use opendrive_app::runtime::fonts::DiscoveredFont;
use opendrive_core::Settings;
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

fn find_text_pos(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Pos2> {
    fn walk(s: &Shape, needle: &str) -> Option<Pos2> {
        match s {
            Shape::Text(t) if t.galley.text() == needle => Some(s.visual_bounding_rect().center()),
            Shape::Vec(v) => v.iter().find_map(|x| walk(x, needle)),
            _ => None,
        }
    }
    shapes.iter().rev().find_map(|cs| walk(&cs.shape, needle))
}

fn draw_font(
    ctx: &egui::Context,
    fonts: &[DiscoveredFont],
    draft: &mut FontPickerDraft,
    events: Vec<Event>,
) -> (Vec<UiAction>, Vec<egui::epaint::ClippedShape>) {
    let (mut actions, pal) = (Vec::new(), palette(ThemeMode::DarkStudio));
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(720.0, 420.0))),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            render_font_section(ui, &pal, Some("Inter"), fonts, draft, &mut actions);
        });
    });
    out.textures_delta.clear();
    (actions, out.shapes)
}

fn click_label(
    ctx: &egui::Context,
    fonts: &[DiscoveredFont],
    draft: &mut FontPickerDraft,
    label: &str,
) -> Vec<UiAction> {
    let (_, shapes) = draw_font(ctx, fonts, draft, Vec::new());
    let pos = find_text_pos(&shapes, label).expect("label in font UI");
    [true, false]
        .into_iter()
        .flat_map(|pressed| {
            let btn = Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            draw_font(ctx, fonts, draft, vec![Event::PointerMoved(pos), btn]).0
        })
        .collect()
}

fn no_name_ttf() -> Vec<u8> {
    let mut b = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    b.extend_from_slice(b"head");
    b.extend_from_slice(&[0u8; 12]);
    b
}

#[test]
fn font_import_http_fallbacks_and_spawn_repaint_work() {
    let nn = no_name_ttf();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let _srv = std::thread::spawn(move || {
        let rep = |code: &str, ct: &str, body: Vec<u8>| {
            let h = format!("HTTP/1.1 {code}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            (h, body)
        };
        for mut s in listener.incoming().flatten() {
            let mut b = [0u8; 1024];
            let n = s.read(&mut b).unwrap_or(0);
            let raw = String::from_utf8_lossy(&b[..n]).into_owned();
            let first = raw.lines().next().unwrap_or("");
            let path = first.split_whitespace().nth(1).unwrap_or("/");
            let css =
                |f: &str| format!("@font-face {{ src: url(http://{addr}/{f}); }}").into_bytes();
            let (hdr, body) = match path {
                "/css-empty" => rep(
                    "200 OK",
                    "text/css",
                    b"@font-face { font-family: 'X'; }".to_vec(),
                ),
                "/css-403" => rep("200 OK", "text/css", css("font-403.ttf")),
                "/css-500" => rep("200 OK", "text/css", css("font-500.ttf")),
                p if p == "/font-403.ttf" || p.starts_with("/g-403") => {
                    rep("403 Forbidden", "text/plain", vec![])
                }
                "/font-500.ttf" => rep("500 Internal Server Error", "text/plain", vec![]),
                _ => rep("200 OK", "font/ttf", nn.clone()),
            };
            let _ = s.write_all(hdr.as_bytes());
            let _ = s.write_all(&body);
        }
    });

    let tmp = std::env::temp_dir().join(format!("od-fimp-cov-{}", Uuid::new_v4()));
    assert!(import_font_from_url_with_policy("not a url", &tmp, false).is_err());
    for p in ["css-empty", "css-403", "css-500"] {
        assert!(
            import_font_from_url_with_policy(&format!("http://{addr}/{p}"), &tmp, false).is_err()
        );
    }
    let (fam, saved) =
        import_font_from_url_with_policy(&format!("http://{addr}/"), &tmp, false).unwrap();
    assert_eq!(fam, "CDN Font");
    assert!(saved.exists());
    assert!(
        import_google_font_family("X", &format!("http://{addr}/g-403"), &tmp)
            .unwrap_err()
            .contains("denied")
    );

    let h = spawn_font_import(
        "http://127.0.0.1:1/bad.ttf".into(),
        format!("http://{addr}/g-403"),
        tmp.clone(),
        Some(egui::Context::default()),
    );
    assert!(h.rx.recv().unwrap().is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn font_actions_errors_and_font_ui_popup_default_and_importing_work() {
    let tmp = std::env::temp_dir().join(format!("od-fact-cov-{}", Uuid::new_v4()));
    let (data_dir, cfg_file) = (tmp.join("data"), tmp.join("cfg_is_file"));
    std::fs::create_dir_all(data_dir.join("fonts")).unwrap();
    std::fs::write(&cfg_file, b"not-a-dir").unwrap();
    std::fs::write(data_dir.join("fonts").join("SeedFont.ttf"), no_name_ttf()).unwrap();

    let engine = Arc::new(SyncEngine::new(Store::open_in_memory().unwrap()));
    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), engine, Some(cfg_file));
    let eof_srv = TcpListener::bind("127.0.0.1:0").unwrap();
    let eof_addr = eof_srv.local_addr().unwrap();
    let _eof_thread = std::thread::spawn(move || {
        if let Ok((s, _)) = eof_srv.accept() {
            drop(s);
        }
    });
    let ep = AuthEndpoints {
        google_fonts_css_base: format!("http://{eof_addr}/css2"),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge.with_data_dir(data_dir), ep);
    let ctx = egui::Context::default();

    app.apply_action(UiAction::SelectUiFont("SeedFont".into()));
    assert!(app.font_draft.feedback_is_err);
    app.apply_action(UiAction::ResetUiFont);
    assert!(app.font_draft.feedback_is_err);

    app.font_draft.import_input = "UnreachableFont".into();
    app.apply_action(UiAction::ImportUiFont);
    for _ in 0..80 {
        let _ = run_frame(&ctx, &mut app, Vec::new());
        if !app.font_draft.importing {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    assert!(!app.font_draft.importing && app.font_draft.feedback_is_err);

    let fonts = vec![DiscoveredFont {
        family: "Inter".into(),
        path: PathBuf::from("inter.ttf"),
        imported: true,
    }];
    let mut busy = FontPickerDraft {
        import_input: "Lato".into(),
        feedback: Some("Imported OK".into()),
        importing: true,
        ..Default::default()
    };
    assert!(find_text_pos(
        &draw_font(&ctx, &fonts, &mut busy, vec![]).1,
        "Importing..."
    )
    .is_some());

    let mut draft = FontPickerDraft::default();
    let _ = click_label(&ctx, &fonts, &mut draft, "Inter");
    let acts = click_label(&ctx, &fonts, &mut draft, "OpenDrive Default");
    assert!(acts.iter().any(|a| matches!(a, UiAction::ResetUiFont)));
    assert!(!egui::Popup::is_any_open(&ctx));

    let _ = click_label(
        &ctx,
        &fonts,
        &mut draft,
        "Google Font, public https:// CDN URL, or .ttf/.otf path",
    );
    draft.import_input = "Lato".into();
    let kev = |key| Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let (enter_acts, _) = draw_font(&ctx, &fonts, &mut draft, vec![kev(Key::Enter)]);
    assert!(enter_acts
        .iter()
        .any(|a| matches!(a, UiAction::ImportUiFont)));
    let _ = std::fs::remove_dir_all(&tmp);
}
