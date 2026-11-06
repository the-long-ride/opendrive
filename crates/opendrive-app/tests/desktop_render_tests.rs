//! Headless egui rendering tests covering Dark/Light modes, auth drawer, sliders, switches, icons, and text contrast.

mod common_auth_helpers;

use common_auth_helpers::poll_until;
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use opendrive_app::design::formatters::{ConnectAuthMode, BANDWIDTH_50M};
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::design::widgets::{paint_icon, pill_button, ButtonVariant, IconKind};
use opendrive_app::design::widgets_controls::{
    setting_switch_row, tactile_step_slider, tactile_switch,
};
use opendrive_app::desktop::{ControlCenterApp, NoticeTone, UiAction, UiNotice};
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_core::{Account, AccountStatus, DocsMode, Settings};
use std::sync::Arc;
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
        storage_used: 1_073_741_824,
        storage_quota: 16_106_127_360,
        avatar_url: None,
    }
}

fn render_size(ctx: &egui::Context, size: Vec2, app: &mut ControlCenterApp) -> usize {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out.shapes.len()
}

#[test]
fn headless_egui_renders_dark_light_viewports_controls_and_icons() {
    let (bridge, _rx) = Bridge::new(Settings::default());
    let endpoints = AuthEndpoints {
        browser_opener: Arc::new(|_| Ok(())),
        folder_opener: Arc::new(|_| Ok(())),
        ..Default::default()
    };
    let mut app = ControlCenterApp::with_auth_endpoints(bridge, endpoints);
    let ctx = egui::Context::default();

    for mode in [ThemeMode::ColorPop, ThemeMode::DarkStudio] {
        app.theme_mode = mode;
        app.drawer_open = false;
        app.settings_open = false;
        app.state.accounts.clear();
        assert!(render_size(&ctx, Vec2::new(960.0, 760.0), &mut app) > 10);

        app.toggle_drawer(Some(true));
        app.apply_action(UiAction::SetAuthMode(ConnectAuthMode::BrowserOAuth));
        app.draft.client_id = "cid-demo".into();
        app.draft.client_secret = "csec-demo".into();
        app.draft.show_secret = false;
        app.form_error = Some("Enter your Google OAuth Client ID.".into());
        app.notice = Some(UiNotice {
            tone: NoticeTone::Error,
            message: "Disk full".into(),
        });
        assert!(render_size(&ctx, Vec2::new(960.0, 760.0), &mut app) > 20);

        app.apply_action(UiAction::ToggleSecretVisibility);
        assert!(render_size(&ctx, Vec2::new(960.0, 760.0), &mut app) > 20);

        app.apply_action(UiAction::StartBrowserOAuth {
            open_browser: false,
            copy_link: false,
        });
        poll_until(&mut app, |a| {
            a.auth_session
                .as_ref()
                .and_then(|s| s.auth_url.as_ref())
                .is_some()
        });
        app.auth_session.as_mut().unwrap().auth_url = None;
        assert!(render_size(&ctx, Vec2::new(960.0, 760.0), &mut app) > 20);
        app.auth_session.as_mut().unwrap().auth_url = Some(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id=cid&state=st&extra=1234567890abcdef"
                .into(),
        );
        assert!(render_size(&ctx, Vec2::new(960.0, 760.0), &mut app) > 20);
        app.apply_action(UiAction::CancelAuth);

        app.apply_action(UiAction::SetAuthMode(ConnectAuthMode::ServiceAccount));
        assert!(render_size(&ctx, Vec2::new(960.0, 760.0), &mut app) > 20);

        app.state.accounts = vec![
            sample_account("Work", "work@example.com", AccountStatus::Connected, false),
            sample_account("Archive", "", AccountStatus::Paused, true),
            sample_account(
                "Personal",
                "p@example.com",
                AccountStatus::NeedsReauth,
                false,
            ),
            sample_account("Broken", "b@example.com", AccountStatus::Error, false),
        ];
        app.state.stats.errors = 2;
        app.state.stats.cache_bytes = 9_500_000_000;
        app.state.settings.cache_max_size = 10_000_000_000;
        app.state.settings.bandwidth_limit = Some(BANDWIDTH_50M);
        app.state.settings.autostart = true;
        app.state.settings.virtual_drive_enabled = true;
        app.state.settings.docs_mode = DocsMode::ExportOffice;
        app.notice = Some(UiNotice {
            tone: NoticeTone::Ok,
            message: "Ready".into(),
        });
        assert!(render_size(&ctx, Vec2::new(1120.0, 760.0), &mut app) > 40);
        assert!(render_size(&ctx, Vec2::new(610.0, 480.0), &mut app) > 40);
        app.apply_action(UiAction::StartEditAlias(
            app.state.accounts[0].id.to_string(),
        ));
        assert!(render_size(&ctx, Vec2::new(720.0, 620.0), &mut app) > 45);
        app.apply_action(UiAction::CancelAlias);
        app.settings_open = true;
        assert!(render_size(&ctx, Vec2::new(960.0, 720.0), &mut app) > 50);
        app.settings_open = false;
    }

    app.apply_action(UiAction::DismissNotice);
    let icons = [
        IconKind::Brand,
        IconKind::Folder,
        IconKind::Plus,
        IconKind::Close,
        IconKind::Pause,
        IconKind::Play,
        IconKind::Lock,
        IconKind::Unlock,
        IconKind::Trash,
        IconKind::Purge,
        IconKind::Check,
        IconKind::Alert,
        IconKind::Refresh,
        IconKind::Sun,
        IconKind::Moon,
        IconKind::Copy,
        IconKind::External,
        IconKind::Key,
        IconKind::Settings,
        IconKind::Minimize,
        IconKind::Maximize,
        IconKind::Restore,
        IconKind::Edit,
        IconKind::Heart,
        IconKind::Star,
    ];
    let pal = palette(ThemeMode::ColorPop);
    let slider_ctx = egui::Context::default();
    let mut slider_hit = Pos2::new(300.0, 42.0);
    let mut first_out = slider_ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 320.0))),
            ..Default::default()
        },
        |ui| {
            let _ = tactile_step_slider(ui, &pal, 0, 5, "Bandwidth", "Unlimited");
            let rect = ui.min_rect();
            slider_hit = Pos2::new(rect.right() - 20.0, rect.bottom() - 12.0);
        },
    );
    first_out.textures_delta.clear();

    let mut slider_step = None;
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 320.0))),
        events: vec![
            Event::PointerMoved(slider_hit),
            Event::PointerButton {
                pos: slider_hit,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos: slider_hit,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
        ..Default::default()
    };
    let mut out = slider_ctx.run_ui(raw, |ui| {
        slider_step = tactile_step_slider(ui, &pal, 0, 5, "Bandwidth", "Unlimited");
        assert_eq!(tactile_step_slider(ui, &pal, 0, 1, "Single", "Fixed"), None);
        for icon in icons {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
            paint_icon(ui.painter(), rect, icon, pal.text_primary);
        }
        let _ = tactile_switch(ui, &pal, true, true);
        let _ = tactile_switch(ui, &pal, false, false);
        let _ = setting_switch_row(ui, &pal, true, "Autostart", "Start with OS", "Enabled");
        let _ = pill_button(
            ui,
            &pal,
            Some(IconKind::Plus),
            "Primary",
            ButtonVariant::Primary,
            false,
            true,
        );
        let _ = pill_button(
            ui,
            &pal,
            None,
            "Dock",
            ButtonVariant::TactilePill,
            true,
            true,
        );
        let _ = pill_button(
            ui,
            &pal,
            None,
            "Disabled",
            ButtonVariant::Danger,
            true,
            false,
        );
        let _ = pill_button(
            ui,
            &pal,
            None,
            "Selected",
            ButtonVariant::Toggle { selected: true },
            true,
            true,
        );
    });
    out.textures_delta.clear();
    assert!(!out.shapes.is_empty());
    assert!(slider_step.is_some());
}

#[test]
fn keyboard_zoom_and_tray_summary_updater_work() {
    use eframe::egui::Key;
    use opendrive_platform::{desktop_tray_menu, TrayAction};

    let menu = desktop_tray_menu();
    assert_eq!(menu[0].action, TrayAction::Summary);
    assert!(!menu[0].enabled);

    let (bridge, _rx) = Bridge::new(Settings::default());
    let summary = Arc::new(std::sync::Mutex::new(String::new()));
    let summary_ref = Arc::clone(&summary);
    let mut app =
        ControlCenterApp::new(bridge).with_tray_summary_updater(std::rc::Rc::new(move |txt| {
            *summary_ref.lock().unwrap() = txt.to_owned();
        }));
    app.state.accounts = vec![sample_account(
        "Work",
        "work@example.com",
        AccountStatus::Connected,
        false,
    )];
    let ctx = egui::Context::default();
    let mut send_key = |key: Key| {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 720.0))),
            events: vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            }],
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
        out.textures_delta.clear();
        let _ = render_size(&ctx, Vec2::new(960.0, 720.0), &mut app);
    };
    send_key(Key::Equals);
    assert!(ctx.zoom_factor() > 1.05, "zoom={}", ctx.zoom_factor());
    send_key(Key::Minus);
    assert!(
        (ctx.zoom_factor() - 1.0).abs() < 0.05,
        "zoom={}",
        ctx.zoom_factor()
    );
    send_key(Key::Equals);
    send_key(Key::Num0);
    assert!((ctx.zoom_factor() - 1.0).abs() < 0.01);
    let text = summary.lock().unwrap().clone();
    assert!(text.contains("Sync Active"));
    assert!(text.contains("% used"));
}
