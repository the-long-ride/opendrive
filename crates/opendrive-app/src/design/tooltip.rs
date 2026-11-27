//! Viewport-aware smart tooltips painted on the Tooltip layer above all panels and menus.

use crate::design::formatters::StatusTone;
use crate::design::icons::IconKind;
use crate::design::theme::ThemePalette;
use crate::design::widgets::ButtonVariant;
use eframe::egui::{
    self, CornerRadius, FontId, Id, LayerId, Order, Pos2, Rect, Response, Stroke, StrokeKind, Vec2,
};

pub const TOOLTIP_MARGIN: f32 = 8.0;
pub const TOOLTIP_GAP: f32 = 6.0;
pub const TOOLTIP_SHADOW: f32 = 2.0;
const TOOLTIP_MAX_WIDTH: f32 = 280.0;
const TOOLTIP_PAD: Vec2 = Vec2::new(10.0, 6.0);
const OPEN_SETTINGS_TIP: &str = "Open application preferences and sync settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooltipPlacement {
    Bottom,
    Top,
}

/// Computes a tooltip rectangle that defaults below `target`, flips above when bottom clipping
/// would occur, and shifts horizontally so 100% of the tooltip + 2px shadow remains in `viewport`.
pub fn compute_smart_tooltip_rect(
    target: Rect,
    tip_size: Vec2,
    viewport: Rect,
) -> (Rect, TooltipPlacement) {
    let safe_left = (viewport.left() + TOOLTIP_MARGIN).min(viewport.right());
    let safe_right = (viewport.right() - TOOLTIP_MARGIN).max(safe_left);
    let safe_top = (viewport.top() + TOOLTIP_MARGIN).min(viewport.bottom());
    let safe_bottom = (viewport.bottom() - TOOLTIP_MARGIN).max(safe_top);

    let eff_w = tip_size.x + TOOLTIP_SHADOW;
    let eff_h = tip_size.y + TOOLTIP_SHADOW;

    let bottom_y = target.bottom() + TOOLTIP_GAP;
    let top_y = target.top() - TOOLTIP_GAP - tip_size.y;
    let max_y = (safe_bottom - eff_h).max(safe_top);

    let (placement, y) = if bottom_y + eff_h <= safe_bottom {
        (TooltipPlacement::Bottom, bottom_y)
    } else if top_y >= safe_top {
        (TooltipPlacement::Top, top_y)
    } else {
        let space_below = safe_bottom - bottom_y;
        let space_above = (target.top() - TOOLTIP_GAP) - safe_top;
        if space_above > space_below {
            (TooltipPlacement::Top, top_y.clamp(safe_top, max_y))
        } else {
            (TooltipPlacement::Bottom, bottom_y.clamp(safe_top, max_y))
        }
    };

    let max_x = (safe_right - eff_w).max(safe_left);
    let x = (target.center().x - tip_size.x * 0.5).clamp(safe_left, max_x);
    (Rect::from_min_size(Pos2::new(x, y), tip_size), placement)
}

/// Paints a non-interactive Neo-Cel tooltip for `target` on `Order::Tooltip`.
pub fn show_smart_tooltip(ctx: &egui::Context, pal: &ThemePalette, target: Rect, text: &str) {
    if text.is_empty() || !target.is_positive() {
        return;
    }
    let viewport = ctx.content_rect();
    if !viewport.is_positive() {
        return;
    }

    let safe_w = (viewport.width() - TOOLTIP_MARGIN * 2.0 - TOOLTIP_SHADOW).max(80.0);
    let max_text_w =
        (TOOLTIP_MAX_WIDTH - TOOLTIP_PAD.x * 2.0).min((safe_w - TOOLTIP_PAD.x * 2.0).max(40.0));
    let (bg, border, fg) = if pal.is_dark {
        (pal.card_alt, pal.pop_cyan, pal.text_primary)
    } else {
        (pal.card_bg, pal.tactile_ink, pal.text_primary)
    };

    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("od_smart_tooltip")));
    let galley = painter.layout(text.to_owned(), FontId::proportional(11.5), fg, max_text_w);
    let tip_size = galley.size() + TOOLTIP_PAD * 2.0;
    let (rect, _placement) = compute_smart_tooltip_rect(target, tip_size, viewport);

    let radius = CornerRadius::same(7);
    painter.rect_filled(
        rect.translate(Vec2::splat(TOOLTIP_SHADOW)),
        radius,
        pal.tactile_ink,
    );
    painter.rect(
        rect,
        radius,
        bg,
        Stroke::new(1.5, border),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + TOOLTIP_PAD, galley, fg);
}

/// Shows a smart tooltip anchored to `resp` when hovered.
pub fn attach_smart_tooltip(resp: &Response, ui: &egui::Ui, pal: &ThemePalette, text: &str) {
    if !resp.hovered() {
        return;
    }
    let clipped = resp.rect.intersect(ui.clip_rect());
    let anchor = if clipped.is_positive() {
        clipped
    } else {
        resp.rect
    };
    show_smart_tooltip(ui.ctx(), pal, anchor, text);
}

fn known_button_description(
    icon: Option<IconKind>,
    label: &str,
    variant: ButtonVariant,
) -> Option<&'static str> {
    match label {
        "" => match icon {
            Some(IconKind::Close) if variant == ButtonVariant::Danger => Some("Close window"),
            Some(IconKind::Close) => Some("Close"),
            Some(IconKind::Minimize) => Some("Minimize window to taskbar"),
            Some(IconKind::Maximize) => Some("Maximize window"),
            Some(IconKind::Restore) => Some("Restore window size"),
            Some(IconKind::Refresh) => Some("Refresh folder contents from Google Drive"),
            Some(IconKind::Pause) => Some("Pause syncing across all accounts"),
            Some(IconKind::Play) => Some("Resume syncing across all accounts"),
            Some(IconKind::Settings) => Some(OPEN_SETTINGS_TIP),
            Some(IconKind::Terminal) => Some("Toggle Terminal & App Logs view"),
            Some(IconKind::HelpBook) => Some("Open AI Support Prompt & Help guide"),
            Some(IconKind::Grid) => Some("Switch to grid view"),
            Some(IconKind::List) => Some("Switch to list view"),
            Some(IconKind::ArrowLeft) => Some("Go back"),
            Some(IconKind::ChevronUp) => Some("Move account up"),
            Some(IconKind::ChevronDown) => Some("Move account down"),
            _ => None,
        },
        "Color" => Some("Switch to Color Pop light theme"),
        "Dark" => Some("Switch to Dark Studio theme"),
        "Explore" => Some("Browse files and folders in Cloud Explorer"),
        "Dashboard" => Some("Return to Connected Accounts dashboard"),
        "Open Folder" => Some("Open local OpenDrive mount folder"),
        "Open Settings" => Some(OPEN_SETTINGS_TIP),
        "New Account" | "Connect" | "Add First Account" => {
            Some("Connect a new Google Drive account")
        }
        "Open" if variant == ButtonVariant::Primary => Some("Open selected item"),
        "Open" => Some("Open item or local mount folder"),
        "Alias" => Some("Edit display alias for this account"),
        "Remove" => Some("Disconnect this account from OpenDrive"),
        "Sign In" => Some("Complete Google sign-in for this account"),
        "Credentials" => Some("Update OAuth Client ID, Client Secret, or Service Account JSON"),
        "Read-Write" => Some("Account allows uploads and edits (click for Read-Only)"),
        "Read-Only" => Some("Account is read-only (click to allow uploads and edits)"),
        "Save" => Some("Save changes"),
        "Cancel" => Some("Cancel and discard changes"),
        "Close" => Some("Close this panel"),
        "Done" => Some("Save and close settings"),
        "Retry" => Some("Retry the failed operation"),
        "Dismiss" => Some("Dismiss this notification"),
        "Import Backup" => Some("Import accounts from an encrypted backup vault"),
        "Setup Guide" => Some("View Google OAuth and Service Account setup steps"),
        "Browser Sign-In" => Some("Authenticate via Google OAuth in your browser"),
        "Service Account JSON (No Login)" => Some("Authenticate using a Google Service Account"),
        "Show" => Some("Reveal hidden value"),
        "Hide" => Some("Hide sensitive value"),
        "Copy Link" => Some("Copy sign-in link to clipboard"),
        "Open Browser" => Some("Open Google sign-in URL in your browser"),
        "Cancel Sign-In" => Some("Cancel the active browser sign-in session"),
        "Sign in with Google" => Some("Start browser sign-in for Google Drive"),
        "Connect Service Account" => Some("Connect account using Service Account JSON"),
        "Reset Font" => Some("Restore the default OpenDrive UI font"),
        "Import" => Some("Validate and import custom font family or file"),
        "Export Backup" => Some("Export connected accounts to an encrypted vault"),
        "Browse" => Some("Browse for a folder or file on disk"),
        "Add" => Some("Add this rule to the list"),
        "Check Now" => Some("Check GitHub for newer OpenDrive releases"),
        "Open Google Cloud Console" => Some("Open Google Cloud Console in your browser"),
        "Purge Unpinned Cache" => Some("Delete unpinned cached files to free local disk space"),
        "My Drive" => Some("Navigate to My Drive root"),
        "Shared with me" => Some("Navigate to Shared with me"),
        "Upload" => Some("Upload local files or a folder into this Drive folder"),
        "Upload Files" | "Sync Up Files" => Some("Upload local files into this Drive folder"),
        "Upload Folder" | "Sync Up Folder" => Some("Upload a local folder into this Drive folder"),
        "Sync" => Some("Sync selected items between Google Drive and local disk"),
        "Sync Down (Pin Local)" | "Sync Down" => Some("Download and pin this item locally"),
        "Sync Up" => Some("Upload local changes for this item to Drive"),
        "Sync Up & Clear Local" => Some("Upload local changes to Drive and free local cache"),
        "Copy Stream URL" => Some("Copy direct video stream URL to clipboard"),
        "Rename" => Some("Rename this file or folder on cloud and local"),
        "Copy" => Some("Copy this file or folder to clipboard"),
        "Move" => Some("Cut this file or folder to move to another folder"),
        "Delete" => Some("Delete this file or folder from cloud and local"),
        "Move Here" => Some("Move clipboard item into the current folder"),
        "Paste Copy Here" => Some("Paste copied item into the current folder"),
        s if icon == Some(IconKind::Close) && s.ends_with(" selected") => {
            Some("Clear current selection")
        }
        _ => None,
    }
}

/// Builds a concise, human-readable tooltip for any `pill_button` or context-menu item.
pub fn button_tooltip_text(
    icon: Option<IconKind>,
    label: &str,
    variant: ButtonVariant,
    enabled: bool,
) -> String {
    let base = known_button_description(icon, label, variant).map_or_else(
        || {
            if label.is_empty() {
                "Action button".to_owned()
            } else if icon == Some(IconKind::Folder) {
                format!("Open folder {label}")
            } else if matches!(variant, ButtonVariant::Toggle { .. }) {
                format!("Select {label}")
            } else {
                label.to_owned()
            }
        },
        ToOwned::to_owned,
    );
    if enabled {
        base
    } else {
        format!("{base} (currently unavailable)")
    }
}

/// Maps a status badge label and tone to its representative vector icon and hover tooltip.
pub fn status_badge_spec(label: &str, tone: StatusTone) -> (IconKind, String) {
    match label {
        "This PC Active" => (
            IconKind::Monitor,
            "This PC Active — Mounted in Windows Explorer (This PC)".to_owned(),
        ),
        "Windows" | "macOS" | "Linux" | "Desktop" => (
            IconKind::Monitor,
            format!("{label} Workspace — Local OpenDrive mount active"),
        ),
        "Sync Active" => (
            IconKind::Refresh,
            "Sync Active — Background Google Drive sync is running".to_owned(),
        ),
        "All Paused" => (
            IconKind::Pause,
            "All Paused — Syncing is paused across all accounts".to_owned(),
        ),
        "Attention" => (
            IconKind::Alert,
            "Attention — One or more accounts need sign-in or have sync errors".to_owned(),
        ),
        "Standby" => (
            IconKind::Cloud,
            "Standby — Connect a Google Drive account to start syncing".to_owned(),
        ),
        "Links (.gdoc)" => (
            IconKind::Link,
            "Links (.gdoc) — Google Docs, Sheets & Slides open in browser".to_owned(),
        ),
        "Office Export" => (
            IconKind::DocSheet,
            "Office Export — Google Docs export as .docx, .xlsx & .pptx".to_owned(),
        ),
        "Connected" => (
            IconKind::Check,
            "Connected — Account is authenticated and syncing".to_owned(),
        ),
        "Paused" => (
            IconKind::Pause,
            "Paused — Syncing is paused for this account".to_owned(),
        ),
        "Needs Reauth" => (
            IconKind::Key,
            "Needs Re-authentication — Sign in again to resume syncing".to_owned(),
        ),
        "Error" => (
            IconKind::Alert,
            "Sync Error — Check connection or re-authenticate this account".to_owned(),
        ),
        "Syncing..." => (
            IconKind::Refresh,
            "Syncing — Refreshing folder contents from Google Drive".to_owned(),
        ),
        _ => {
            let icon = match tone {
                StatusTone::Connected => IconKind::Check,
                StatusTone::Paused => IconKind::Pause,
                StatusTone::Warn | StatusTone::Error => IconKind::Alert,
                StatusTone::Standby => IconKind::Cloud,
            };
            (icon, label.to_owned())
        }
    }
}
