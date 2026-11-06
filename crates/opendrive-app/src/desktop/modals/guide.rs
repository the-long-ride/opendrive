//! Step-by-step Google Cloud Console setup guide modal for OAuth 2.0 and Service Accounts.

use crate::design::theme::ThemePalette;
use crate::design::widgets::{bento_card, pill_button, BentoLayerAccent, ButtonVariant, IconKind};
use crate::desktop::UiAction;
use eframe::egui::scroll_area::ScrollBarVisibility;
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Pos2, Rect, RichText,
    ScrollArea, Stroke, Vec2,
};

type Seg = (&'static str, bool);
type StepItem = (&'static str, &'static [&'static [Seg]]);

const OAUTH_STEPS: &[StepItem] = &[
    (
        "Create or select a Google Cloud project",
        &[
            &[
                ("Open ", false),
                ("console.cloud.google.com", true),
                (" and sign in.", false),
            ],
            &[
                ("Click the ", false),
                ("project picker", true),
                (" (top-left) -> ", false),
                ("New Project", true),
                (" -> name it (e.g. ", false),
                ("OpenDrive", true),
                (") -> ", false),
                ("Create", true),
                (".", false),
            ],
        ],
    ),
    (
        "Enable the Google Drive API",
        &[
            &[
                ("Open ", false),
                ("APIs & Services -> Library", true),
                (".", false),
            ],
            &[
                ("Search ", false),
                ("\"Google Drive API\"", true),
                (", open it, and click ", false),
                ("Enable", true),
                (".", false),
            ],
        ],
    ),
    (
        "Configure Google Auth platform / OAuth consent screen",
        &[
            &[
                ("Open ", false),
                ("Google Auth platform", true),
                (" (or ", false),
                ("OAuth consent screen", true),
                (") and click ", false),
                ("Get Started", true),
                (".", false),
            ],
            &[
                ("Set App name (", false),
                ("OpenDrive", true),
                ("), Support email, ", false),
                ("External", true),
                (" audience, and Developer email.", false),
            ],
        ],
    ),
    (
        "Audience & Drive Scope (Important)",
        &[
            &[
                ("Under ", false),
                ("Audience", true),
                (", add your Gmail to ", false),
                ("Test users", true),
                (" (7-day token expiry) OR click ", false),
                ("Publish app -> Confirm", true),
                (".", false),
            ],
            &[
                ("Under ", false),
                ("Data Access -> Add or remove scopes", true),
                (", add ", false),
                ("https://www.googleapis.com/auth/drive", true),
                (" (or ", false),
                (".../drive.readonly", true),
                (") and ", false),
                ("Save", true),
                (".", false),
            ],
        ],
    ),
    (
        "Create a Desktop App OAuth Client",
        &[
            &[
                ("Open ", false),
                ("Clients -> + Create Client", true),
                (" (or ", false),
                (
                    "Credentials -> + Create Credentials -> OAuth client ID",
                    true,
                ),
                (").", false),
            ],
            &[
                ("Choose Application type: ", false),
                ("Desktop app", true),
                (" -> ", false),
                ("Create", true),
                (" (loopback ", false),
                ("http://127.0.0.1", true),
                (" works automatically).", false),
            ],
        ],
    ),
    (
        "Paste credentials & connect in OpenDrive",
        &[
            &[
                ("Copy ", false),
                ("Client ID", true),
                (" (", false),
                ("...apps.googleusercontent.com", true),
                (") & ", false),
                ("Client Secret", true),
                (" (", false),
                ("GOCSPX-...", true),
                (").", false),
            ],
            &[
                ("Click ", false),
                ("Sign in with Google", true),
                (" (or ", false),
                ("Copy Link", true),
                ("). If unverified warning appears, click ", false),
                ("Advanced -> Continue", true),
                (".", false),
            ],
        ],
    ),
];

const SERVICE_ACCOUNT_STEPS: &[StepItem] = &[
    (
        "Enable Google Drive API",
        &[&[
            ("Use ", false),
            ("Option A Steps 1-2", true),
            (" so ", false),
            ("Google Drive API", true),
            (" is enabled in your project.", false),
        ]],
    ),
    (
        "Create a Service Account & JSON Key",
        &[
            &[
                ("Go to ", false),
                (
                    "IAM & Admin -> Service Accounts -> + Create Service Account",
                    true,
                ),
                (" -> ", false),
                ("Done", true),
                (".", false),
            ],
            &[
                ("Open it -> ", false),
                ("Keys -> Add key -> Create new key -> JSON -> Create", true),
                (" to download the ", false),
                (".json", true),
                (" file.", false),
            ],
        ],
    ),
    (
        "Share Drive folders with client_email",
        &[
            &[
                ("Copy ", false),
                ("client_email", true),
                (" (", false),
                ("...@<project>.iam.gserviceaccount.com", true),
                (") from the JSON.", false),
            ],
            &[
                ("In ", false),
                ("drive.google.com", true),
                (", share target folders/Shared Drives with it (", false),
                ("Editor", true),
                (" or ", false),
                ("Viewer", true),
                (").", false),
            ],
        ],
    ),
    (
        "Connect in OpenDrive",
        &[&[
            ("Select ", false),
            ("Service Account JSON (No Login)", true),
            (", drop/paste the JSON, and click ", false),
            ("Connect Service Account", true),
            (".", false),
        ]],
    ),
];

const TROUBLESHOOTING_TIPS: &[StepItem] = &[
    (
        "Missing scope / insufficient permissions (403)",
        &[&[
            ("Open ", false),
            (
                "Google Auth platform -> Data Access -> Add or remove scopes",
                true,
            ),
            (", check ", false),
            ("https://www.googleapis.com/auth/drive", true),
            (" (or ", false),
            (".../drive.readonly", true),
            ("), ", false),
            ("Save", true),
            (", then re-auth.", false),
        ]],
    ),
    (
        "403 access_denied (app in Testing)",
        &[&[
            ("Add your Gmail under ", false),
            ("Audience -> Test users", true),
            (", or click ", false),
            ("Publish app", true),
            (".", false),
        ]],
    ),
    (
        "redirect_uri_mismatch",
        &[&[
            ("Ensure OAuth client Application type is ", false),
            ("Desktop app", true),
            (", not Web application.", false),
        ]],
    ),
    (
        "Multiple Google accounts",
        &[&[
            ("One ", false),
            ("Client ID & Client Secret", true),
            (" can authorize multiple personal Google accounts.", false),
        ]],
    ),
];

fn build_rich_line(pal: &ThemePalette, prefix: &str, segs: &[Seg]) -> LayoutJob {
    let mut job = LayoutJob::default();
    let normal = TextFormat {
        font_id: FontId::proportional(12.0),
        color: pal.text_muted,
        ..Default::default()
    };
    let hi = TextFormat {
        font_id: FontId::monospace(11.5),
        color: pal.text_primary,
        background: pal.accent_blue_tint,
        ..Default::default()
    };
    job.append(prefix, 0.0, normal.clone());
    for &(text, highlight) in segs {
        job.append(
            text,
            0.0,
            if highlight {
                hi.clone()
            } else {
                normal.clone()
            },
        );
    }
    job
}

fn render_guide_section(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    title: &str,
    badge: &str,
    steps: &[StepItem],
) {
    Frame::new()
        .fill(pal.card_alt)
        .stroke(Stroke::new(1.5, pal.card_border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(title)
                        .size(14.0)
                        .strong()
                        .color(pal.text_primary),
                );
                if !badge.is_empty() {
                    ui.label(
                        RichText::new(format!("· {badge}"))
                            .size(12.0)
                            .color(pal.accent_blue),
                    );
                }
            });
            ui.add_space(8.0);
            for (idx, (heading, bullets)) in steps.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("{}.", idx + 1))
                            .size(12.5)
                            .strong()
                            .color(pal.accent_blue),
                    );
                    ui.label(
                        RichText::new(*heading)
                            .size(12.5)
                            .strong()
                            .color(pal.text_primary),
                    );
                });
                for bullet in *bullets {
                    ui.add(egui::Label::new(build_rich_line(pal, "   • ", bullet)).wrap());
                }
                ui.add_space(6.0);
            }
        });
}

pub fn render_cloud_guide_modal(
    ctx: &egui::Context,
    pal: &ThemePalette,
    top_bar_bottom: f32,
    actions: &mut Vec<UiAction>,
) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.push(UiAction::ToggleGuide(Some(false)));
        return;
    }
    let screen = ctx.content_rect();
    let top_y = top_bar_bottom.clamp(screen.min.y, screen.max.y);
    let overlay = Rect::from_min_max(Pos2::new(screen.min.x, top_y), screen.max);
    egui::Area::new(Id::unique("cloud_guide_backdrop"))
        .fixed_pos(overlay.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(overlay.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(145));
            if resp.clicked() {
                actions.push(UiAction::ToggleGuide(Some(false)));
            }
        });

    let modal_w = (overlay.width() - 64.0).clamp(320.0, 680.0);
    let center_offset_y = (top_y - screen.min.y) * 0.5;
    let max_body_h = (overlay.height() - 64.0 - 156.0).clamp(120.0, 480.0);
    egui::Area::new(Id::unique("cloud_guide_card"))
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, center_offset_y))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(modal_w);
            ui.set_min_width(modal_w);
            bento_card(
                ui,
                pal,
                BentoLayerAccent::Dual,
                "Google Cloud Console Setup Guide",
                Some("Highlighted badges mark values, menus, and buttons to locate in Google Cloud Console."),
                None,
                |ui| {
                    ScrollArea::vertical()
                        .max_height(max_body_h)
                        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            render_guide_section(ui, pal, "Option A — Browser Sign-In (OAuth 2.0 Desktop App)", "Recommended for personal & Google Workspace Drive", OAUTH_STEPS);
                            ui.add_space(10.0);
                            render_guide_section(ui, pal, "Option B — Service Account JSON (No-Login Mode)", "Shared Drives & headless automation", SERVICE_ACCOUNT_STEPS);
                            ui.add_space(10.0);
                            render_guide_section(ui, pal, "Quick Troubleshooting", "", TROUBLESHOOTING_TIPS);
                        });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if pill_button(ui, pal, Some(IconKind::External), "Open Google Cloud Console", ButtonVariant::Primary, true, true) {
                            actions.push(UiAction::OpenCloudConsole);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if pill_button(ui, pal, Some(IconKind::Close), "Close", ButtonVariant::Default, true, true) {
                                actions.push(UiAction::ToggleGuide(Some(false)));
                            }
                        });
                    });
                },
            );
        });
}
