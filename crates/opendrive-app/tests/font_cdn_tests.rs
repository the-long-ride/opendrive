//! Tests for public HTTPS CDN font URL validation, downloads, CSS extraction, error mapping, and UI notice.

mod common_auth_helpers;

use common_auth_helpers::run_frame;
use eframe::egui;
use opendrive_app::desktop::{ControlCenterApp, UiAction};
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::font_import::{import_font_from_url, import_font_from_url_with_policy};
use opendrive_app::runtime::font_url::{
    detect_font_extension, is_url_like_input, validate_public_font_url,
};
use opendrive_core::Settings;
use std::io::{Read, Write};
use std::net::TcpListener;
use uuid::Uuid;

fn bundled_ttf() -> Vec<u8> {
    let defs = egui::FontDefinitions::default();
    defs.font_data
        .values()
        .next()
        .expect("egui bundled font")
        .font
        .as_ref()
        .as_ref()
        .to_vec()
}

#[test]
fn validate_public_font_url_and_format_helpers_work() {
    assert!(is_url_like_input("https://cdn.jsdelivr.net/font.ttf"));
    assert!(is_url_like_input("http://example.com/a.otf"));
    assert!(!is_url_like_input("Inter"));
    assert!(!is_url_like_input(r"C:\Fonts\Inter.ttf"));

    assert_eq!(detect_font_extension(b"OTTO\x00"), "otf");
    assert_eq!(detect_font_extension(b"ttcf\x00"), "ttc");
    assert_eq!(detect_font_extension(b"\x00\x01\x00\x00"), "ttf");

    assert!(validate_public_font_url("https://cdn.jsdelivr.net/npm/inter/Inter.ttf").is_ok());
    assert!(validate_public_font_url("https://8.8.8.8/font.ttf").is_ok());

    for bad in [
        "not a url",
        "http://cdn.example.com/Inter.ttf",
        "file:///C:/Fonts/Inter.ttf",
        "https://user:pass@cdn.example.com/Inter.ttf",
        "https://localhost/Inter.ttf",
        "https://dev.localhost/Inter.ttf",
        "https://printer.local/Inter.ttf",
        "https://font.internal/Inter.ttf",
        "https://nas.lan/Inter.ttf",
        "https://router.home.arpa/Inter.ttf",
        "https://intranet/Inter.ttf",
        "https://127.0.0.1/Inter.ttf",
        "https://10.0.0.1/Inter.ttf",
        "https://172.16.0.1/Inter.ttf",
        "https://192.168.1.10/Inter.ttf",
        "https://169.254.169.254/latest",
        "https://100.64.0.1/Inter.ttf",
        "https://[::1]/Inter.ttf",
        "https://[fc00::1]/Inter.ttf",
        "https://[fe80::1]/Inter.ttf",
        "https://[::ffff:127.0.0.1]/Inter.ttf",
    ] {
        assert!(
            validate_public_font_url(bad).is_err(),
            "expected err for {bad}"
        );
    }
    assert!(import_font_from_url("https://127.0.0.1/font.ttf", &std::env::temp_dir()).is_err());
}

#[test]
fn cdn_url_import_downloads_css_and_ttf_and_surfaces_ui_notice() {
    let ttf = bundled_ttf();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let ttf_copy = ttf.clone();
    let _srv = std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut b = [0u8; 2048];
            let n = s.read(&mut b).unwrap_or(0);
            let req = String::from_utf8_lossy(&b[..n]);
            let first = req.lines().next().unwrap_or("");
            if first.contains("GET /direct.ttf") || first.contains("GET /css-font.ttf") {
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: font/ttf\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    ttf_copy.len()
                );
                let _ = s.write_all(hdr.as_bytes());
                let _ = s.write_all(&ttf_copy);
            } else if first.contains("GET /font.css") {
                let css = format!(
                    "@font-face {{ font-family: 'CdnDemo'; src: url(http://{addr}/css-font.ttf); }}"
                );
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    css.len()
                );
                let _ = s.write_all(hdr.as_bytes());
                let _ = s.write_all(css.as_bytes());
            } else if first.contains("GET /private.ttf") {
                let _ = s.write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            } else if first.contains("GET /too-big.ttf") {
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 30000000\r\nConnection: close\r\n\r\n",
                );
            } else {
                let _ = s.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        }
    });

    let tmp = std::env::temp_dir().join(format!("od-cdn-font-{}", Uuid::new_v4()));
    let (f1, p1) =
        import_font_from_url_with_policy(&format!("http://{addr}/direct.ttf"), &tmp, false)
            .unwrap();
    assert!(!f1.is_empty() && p1.exists());
    let (_, p2) =
        import_font_from_url_with_policy(&format!("http://{addr}/font.css"), &tmp, false).unwrap();
    assert!(p2.exists());
    assert!(
        import_font_from_url_with_policy(&format!("http://{addr}/private.ttf"), &tmp, false)
            .unwrap_err()
            .contains("not publicly accessible")
    );
    assert!(
        import_font_from_url_with_policy(&format!("http://{addr}/too-big.ttf"), &tmp, false)
            .unwrap_err()
            .contains("20 MiB")
    );
    assert!(
        import_font_from_url_with_policy(&format!("http://{addr}/missing.ttf"), &tmp, false)
            .is_err()
    );

    // UI notice & background worker URL rejection via Settings -> Import
    let (bridge, _) = Bridge::new(Settings::default());
    let mut app = ControlCenterApp::new(bridge);
    let ctx = egui::Context::default();
    app.apply_action(UiAction::ToggleSettings(Some(true)));
    app.font_draft.import_input = "http://cdn.example.com/bad.ttf".into();
    app.apply_action(UiAction::ImportUiFont);
    for _ in 0..80 {
        let _ = run_frame(&ctx, &mut app, Vec::new());
        if !app.font_draft.importing {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    assert!(!app.font_draft.importing && app.font_draft.feedback_is_err);
    assert!(app
        .font_draft
        .feedback
        .as_deref()
        .unwrap_or("")
        .contains("https://"));

    let _ = std::fs::remove_dir_all(&tmp);
}
