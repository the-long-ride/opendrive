//! Integration tests for optional GitHub release check, semver comparison, ignore-version, and dialog rendering.

use eframe::egui::{self, Pos2, RawInput, Rect, Vec2};
use opendrive_app::desktop::{ControlCenterApp, NoticeTone, UiAction};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::release_check::{is_newer_release, parse_semver};
use opendrive_core::Settings;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_mock_release_server(
    status: &'static str,
    body: &'static str,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        }
    });
    (format!("http://{addr}/releases/latest"), handle)
}

fn pump_frame(ctx: &egui::Context, app: &mut ControlCenterApp) {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
}

#[test]
fn semver_parsing_and_comparison_work() {
    assert_eq!(parse_semver("v0.1.0"), Some((0, 1, 0)));
    assert_eq!(parse_semver("1.2.3-rc1"), Some((1, 2, 3)));
    assert_eq!(parse_semver("bad"), None);
    assert!(!is_newer_release("0.1.0", "v0.1.0"));
    assert!(!is_newer_release("0.2.0", "v0.1.9"));
    assert!(is_newer_release("0.1.0", "v0.2.0"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn optional_release_check_dialog_and_ignore_version_work() {
    let (url, handle) = spawn_mock_release_server(
        "200 OK",
        r#"{"tag_name":"v9.9.9","name":"OpenDrive v9.9.9","html_url":"https://github.com/the-long-ride/opendrive/releases/tag/v9.9.9","body":"New features!"}"#,
    )
    .await;

    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let opened_ref = Arc::clone(&opened);
    let (bridge, _rx) = Bridge::new(Settings::default());
    assert!(!bridge.state_snapshot().settings.check_updates);

    let mut app = ControlCenterApp::with_auth_endpoints(
        bridge,
        AuthEndpoints {
            release_api_url: url,
            browser_opener: Arc::new(move |u| {
                opened_ref.lock().unwrap().push(u.to_string());
                Ok(())
            }),
            ..Default::default()
        },
    );
    let ctx = egui::Context::default();
    pump_frame(&ctx, &mut app);
    assert!(app.available_release.is_none());
    assert!(!app.release_checking);

    app.apply_action(UiAction::CheckReleasesNow);
    for _ in 0..40 {
        pump_frame(&ctx, &mut app);
        if app.available_release.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(
        app.available_release.as_ref().map(|r| r.tag_name.as_str()),
        Some("v9.9.9")
    );
    pump_frame(&ctx, &mut app);

    let release_url = app.available_release.as_ref().unwrap().html_url.clone();
    app.apply_action(UiAction::OpenReleasePage(release_url.clone()));
    assert!(app.available_release.is_none());
    assert_eq!(opened.lock().unwrap().as_slice(), [release_url]);

    app.apply_action(UiAction::IgnoreReleaseVersion("v9.9.9".into()));
    assert_eq!(
        app.state.settings.ignored_release_version.as_deref(),
        Some("v9.9.9")
    );

    app.apply_action(UiAction::ToggleCheckUpdates);
    assert!(app.state.settings.check_updates);
    for _ in 0..40 {
        pump_frame(&ctx, &mut app);
        if !app.release_checking {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(app.available_release.is_none());
    app.apply_action(UiAction::OpenExternalUrl(
        "https://github.com/the-long-ride/#donate",
    ));
    app.apply_action(UiAction::OpenExternalUrl(
        "https://github.com/the-long-ride/opendrive",
    ));
    assert_eq!(opened.lock().unwrap().len(), 3);
    handle.abort();

    let (ok_url, ok_h) = spawn_mock_release_server(
        "200 OK",
        r#"{"tag_name":"v0.1.0","name":"v0.1.0","html_url":"https://example.com","body":""}"#,
    )
    .await;
    let (b2, _rx2) = Bridge::new(Settings::default());
    let mut app2 = ControlCenterApp::with_auth_endpoints(
        b2,
        AuthEndpoints {
            release_api_url: ok_url,
            ..Default::default()
        },
    );
    app2.apply_action(UiAction::CheckReleasesNow);
    for _ in 0..40 {
        pump_frame(&ctx, &mut app2);
        if !app2.release_checking {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(app2.notice.as_ref().map(|n| n.tone), Some(NoticeTone::Ok));
    ok_h.abort();

    let (err_url, err_h) = spawn_mock_release_server("500 Internal Server Error", "{} ").await;
    let (b3, _rx3) = Bridge::new(Settings::default());
    let mut app3 = ControlCenterApp::with_auth_endpoints(
        b3,
        AuthEndpoints {
            release_api_url: err_url,
            ..Default::default()
        },
    );
    app3.apply_action(UiAction::CheckReleasesNow);
    for _ in 0..40 {
        pump_frame(&ctx, &mut app3);
        if !app3.release_checking {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(
        app3.notice.as_ref().map(|n| n.tone),
        Some(NoticeTone::Error)
    );
    err_h.abort();
}
