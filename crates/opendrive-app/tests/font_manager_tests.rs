//! Tests for UI font validation, local & Google Fonts import, font picker UI, and persistence.

mod common_auth_helpers;

use common_auth_helpers::run_frame;
use eframe::egui;
use opendrive_app::design::theme_fonts::apply_ui_font;
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::font_import::{
    extract_ttf_url_from_css, import_google_font_family, import_local_font_file,
};
use opendrive_app::runtime::fonts::{
    discover_fonts_in_dirs, resolve_font_bytes, validate_font_bytes, DiscoveredFont,
};
use opendrive_core::{load_settings, Settings};
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use uuid::Uuid;

fn bundled_valid_ttf() -> Vec<u8> {
    let defs = egui::FontDefinitions::default();
    let font_arc = &defs
        .font_data
        .values()
        .next()
        .expect("egui bundled font")
        .font;
    font_arc.as_ref().as_ref().to_vec()
}

fn spawn_google_fonts_mock(ttf_bytes: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}/css2");
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let first_line = req.lines().next().unwrap_or("");
            if first_line.contains("family=UnknownBad") {
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            } else if first_line.contains("GET /font.ttf") {
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: font/ttf\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    ttf_bytes.len()
                );
                let _ = stream.write_all(hdr.as_bytes());
                let _ = stream.write_all(&ttf_bytes);
            } else {
                let css = format!(
                    "@font-face {{ font-family: 'DemoFont'; src: url(http://{addr}/font.ttf) format('truetype'); }}"
                );
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    css.len()
                );
                let _ = stream.write_all(hdr.as_bytes());
                let _ = stream.write_all(css.as_bytes());
            }
        }
    });
    (base, handle)
}

#[test]
fn font_validation_discovery_and_import_work() {
    let ttf = bundled_valid_ttf();
    let family = validate_font_bytes(&ttf)
        .unwrap()
        .expect("bundled font family");
    assert!(!family.is_empty());

    assert!(validate_font_bytes(b"short").is_err());
    assert!(validate_font_bytes(b"<html>not a font</html>").is_err());
    let zero_tables = [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert!(validate_font_bytes(&zero_tables).is_err());
    let empty_ttc = *b"ttcf\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    assert!(validate_font_bytes(&empty_ttc).is_err());

    let mut valid_ttc = Vec::from(*b"ttcf\x00\x01\x00\x00\x00\x00\x00\x01\x00\x00\x00\x10");
    valid_ttc.extend_from_slice(&ttf);
    assert!(validate_font_bytes(&valid_ttc).is_ok());

    assert_eq!(
        extract_ttf_url_from_css(
            "src: url(https://fonts.gstatic.com/s/inter/a.ttf) format('truetype');"
        ),
        Some("https://fonts.gstatic.com/s/inter/a.ttf".into())
    );
    assert_eq!(
        extract_ttf_url_from_css("src: url(https://example.com/a.woff2);"),
        None
    );

    let tmp = std::env::temp_dir().join(format!("od-font-test-{}", Uuid::new_v4()));
    let sys_dir = tmp.join("sys");
    let imp_dir = tmp.join("imported");
    std::fs::create_dir_all(&sys_dir).unwrap();
    std::fs::write(sys_dir.join("ignored.txt"), b"x").unwrap();
    std::fs::write(sys_dir.join("sys_copy.ttf"), &ttf).unwrap();

    let (imp_fam, saved) = import_local_font_file(&sys_dir.join("sys_copy.ttf"), &imp_dir).unwrap();
    assert_eq!(imp_fam, family);
    assert!(saved.exists());

    let discovered = discover_fonts_in_dirs(std::slice::from_ref(&sys_dir), Some(&imp_dir));
    assert_eq!(discovered.len(), 1);
    assert!(discovered[0].imported);
    assert!(resolve_font_bytes(&family, &discovered).is_some());
    assert!(resolve_font_bytes("missing-font", &discovered).is_none());
    assert!(resolve_font_bytes("", &discovered).is_none());

    let (css_base, _srv) = spawn_google_fonts_mock(ttf.clone());
    let g_dir = tmp.join("gfonts");
    let (gfam, gpath) = import_google_font_family("Demo Font", &css_base, &g_dir).unwrap();
    assert_eq!(gfam, family);
    assert!(gpath.exists());
    assert!(import_google_font_family("UnknownBad", &css_base, &g_dir).is_err());
    assert!(import_google_font_family("   ", &css_base, &g_dir).is_err());

    let ctx = egui::Context::default();
    apply_ui_font(&ctx, Some(&family), Some(ttf));
    apply_ui_font(&ctx, Some(&family), Some(vec![])); // same key -> cached no-op
    apply_ui_font(&ctx, None, None);

    let _ = DiscoveredFont {
        family,
        path: gpath,
        imported: true,
    };
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn settings_font_ui_actions_and_persistence_work() {
    let tmp = std::env::temp_dir().join(format!("od-font-ui-{}", Uuid::new_v4()));
    let cfg_dir = tmp.join("cfg");
    let data_dir = tmp.join("data");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();

    let ttf = bundled_valid_ttf();
    let local_font_path = tmp.join("custom_pick.ttf");
    std::fs::write(&local_font_path, &ttf).unwrap();
    let (css_base, _srv) = spawn_google_fonts_mock(ttf);

    let store = Store::open_in_memory().unwrap();
    let engine = Arc::new(SyncEngine::new(store));
    let (bridge, _rx) = Bridge::new_with_engine(Settings::default(), engine, Some(cfg_dir.clone()));
    let bridge = bridge.with_data_dir(data_dir);

    let pick_path = local_font_path.clone();
    let ep = AuthEndpoints {
        font_open_picker: Arc::new(move || Some(pick_path.clone())),
        google_fonts_css_base: css_base,
        ..Default::default()
    };

    let mut app = ControlCenterApp::with_auth_endpoints(bridge.clone(), ep);
    let ctx = egui::Context::default();

    app.apply_action(UiAction::ToggleSettings(Some(true)));
    let _ = run_frame(&ctx, &mut app, Vec::new());

    // Browse valid local font
    app.apply_action(UiAction::BrowseUiFont);
    let chosen = app
        .state
        .settings
        .ui_font_family
        .clone()
        .expect("font selected");
    let _ = run_frame(&ctx, &mut app, Vec::new());
    assert_eq!(
        load_settings(&cfg_dir).unwrap().ui_font_family.as_deref(),
        Some(chosen.as_str())
    );

    // Select nonexistent font triggers error feedback
    app.apply_action(UiAction::SelectUiFont("NoSuchFontXYZ".into()));
    assert!(app.font_draft.feedback_is_err);

    // Reset Font clears ui_font_family
    app.apply_action(UiAction::ResetUiFont);
    assert!(app.state.settings.ui_font_family.is_none());
    assert!(load_settings(&cfg_dir).unwrap().ui_font_family.is_none());

    // Import empty -> error, then import from mock Google Fonts URL
    app.font_draft.import_input = "  ".into();
    app.apply_action(UiAction::ImportUiFont);
    assert!(app.font_draft.feedback_is_err);

    app.font_draft.import_input = "FreshGoogleFont".into();
    app.apply_action(UiAction::ImportUiFont);
    for _ in 0..80 {
        let _ = run_frame(&ctx, &mut app, Vec::new());
        if !app.font_draft.importing {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(!app.font_draft.importing);
    assert!(app.state.settings.ui_font_family.is_some());

    // Importing already-discovered family selects immediately without network
    app.font_draft.import_input = chosen;
    app.apply_action(UiAction::ImportUiFont);
    assert!(!app.font_draft.importing);

    let _ = std::fs::remove_dir_all(&tmp);
}
