//! Pure formatting, validation, and telemetry summary helpers for the Control Center UI.

use opendrive_core::{Account, AccountStatus};

pub const GIB: u64 = 1024 * 1024 * 1024;
pub const BANDWIDTH_5M: u64 = 5 * 1024 * 1024;
pub const BANDWIDTH_20M: u64 = 20 * 1024 * 1024;
pub const BANDWIDTH_50M: u64 = 50 * 1024 * 1024;
pub const BANDWIDTH_100M: u64 = 100 * 1024 * 1024;

pub const BANDWIDTH_STEPS: [Option<u64>; 5] = [
    None,
    Some(BANDWIDTH_5M),
    Some(BANDWIDTH_20M),
    Some(BANDWIDTH_50M),
    Some(BANDWIDTH_100M),
];

pub const CACHE_STEPS_BYTES: [u64; 5] = [GIB, 5 * GIB, 10 * GIB, 25 * GIB, 50 * GIB];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    Connected,
    Paused,
    Standby,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthSummary {
    pub label: &'static str,
    pub tone: StatusTone,
}

pub use crate::design::avatars::{summarize_total_storage, TotalStorageSummary};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConnectAuthMode {
    #[default]
    BrowserOAuth,
    ServiceAccount,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewAccountDraft {
    pub auth_mode: ConnectAuthMode,
    pub client_id: String,
    pub client_secret: String,
    pub service_account_input: String,
    pub nickname: String,
    pub folder_name: String,
    pub read_only: bool,
    pub show_secret: bool,
}

pub fn bandwidth_to_step(limit: Option<u64>) -> usize {
    match limit {
        None | Some(0) => 0,
        Some(v) if v <= BANDWIDTH_5M => 1,
        Some(v) if v <= BANDWIDTH_20M => 2,
        Some(v) if v <= BANDWIDTH_50M => 3,
        Some(_) => 4,
    }
}

pub fn step_to_bandwidth(step: usize) -> Option<u64> {
    BANDWIDTH_STEPS[step.min(BANDWIDTH_STEPS.len() - 1)]
}

pub fn cache_to_step(bytes: u64) -> usize {
    CACHE_STEPS_BYTES
        .iter()
        .enumerate()
        .min_by_key(|(_, &val)| bytes.abs_diff(val))
        .map_or(2, |(idx, _)| idx)
}

pub fn step_to_cache(step: usize) -> u64 {
    CACHE_STEPS_BYTES[step.min(CACHE_STEPS_BYTES.len() - 1)]
}

pub fn validate_browser_oauth_draft(draft: &NewAccountDraft) -> Option<&'static str> {
    if draft.client_id.trim().is_empty() {
        return Some("Enter your Google OAuth Client ID.");
    }
    if draft.client_secret.trim().is_empty() {
        return Some("Enter your Google OAuth Client Secret.");
    }
    None
}

pub fn validate_service_account_draft(draft: &NewAccountDraft) -> Option<&'static str> {
    if draft.service_account_input.trim().is_empty() {
        return Some("Paste Service Account JSON or enter the .json file path.");
    }
    None
}

pub fn default_folder_name(folder_override: &str, nickname: &str, email: &str) -> String {
    let trimmed_folder = folder_override.trim();
    if !trimmed_folder.is_empty() {
        return trimmed_folder.to_string();
    }
    let trimmed_nick = nickname.trim();
    if !trimmed_nick.is_empty() {
        return trimmed_nick.to_string();
    }
    email
        .split('@')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("drive")
        .to_string()
}

pub use opendrive_platform::format_bytes_compact as format_bytes;

pub fn format_bandwidth(limit: Option<u64>) -> String {
    match limit {
        Some(v) if v > 0 => format!("{}/s", format_bytes(v)),
        _ => "Unlimited".into(),
    }
}

pub fn format_account_storage(used: u64, quota: u64) -> Option<String> {
    if quota > 0 && quota >= used {
        let pct = compute_cache_ratio(used, quota);
        Some(format!(
            "{} / {} ({pct}%)",
            format_bytes(used),
            format_bytes(quota)
        ))
    } else if used > 0 {
        Some(format!("{} used", format_bytes(used)))
    } else {
        None
    }
}

pub fn compute_cache_ratio(used: u64, max: u64) -> u8 {
    if max == 0 {
        return 0;
    }
    let pct = (used as f64 / max as f64) * 100.0;
    pct.round().clamp(0.0, 100.0) as u8
}

pub fn derive_monogram(nickname: &str, email: &str) -> String {
    let source = if !nickname.trim().is_empty() {
        nickname.trim()
    } else {
        email.trim()
    };
    let words: Vec<&str> = source
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if words.len() >= 2 {
        let a = words[0].chars().next().unwrap_or('O');
        let b = words[1].chars().next().unwrap_or('D');
        return format!("{a}{b}").to_uppercase();
    }
    let compact: String = source
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect();
    if compact.is_empty() {
        "OD".into()
    } else {
        compact.to_uppercase()
    }
}

pub fn join_mount_path(root: &str, folder: &str) -> String {
    let clean_root = root.trim_end_matches(['/', '\\']);
    let clean_folder = folder.trim_start_matches(['/', '\\']);
    if clean_root.is_empty() {
        return format!("~/OpenDrive/{clean_folder}");
    }
    let sep = if clean_root.contains('\\') { '\\' } else { '/' };
    format!("{clean_root}{sep}{clean_folder}")
}

pub fn status_label(status: &AccountStatus) -> &'static str {
    match status {
        AccountStatus::Connected => "Connected",
        AccountStatus::Paused => "Paused",
        AccountStatus::NeedsReauth => "Needs Reauth",
        AccountStatus::Error => "Error",
    }
}

pub fn status_tone(status: &AccountStatus) -> StatusTone {
    match status {
        AccountStatus::Connected => StatusTone::Connected,
        AccountStatus::Paused => StatusTone::Paused,
        AccountStatus::NeedsReauth => StatusTone::Warn,
        AccountStatus::Error => StatusTone::Error,
    }
}

pub fn summarize_sync_health(accounts: &[Account]) -> HealthSummary {
    if accounts.is_empty() {
        return HealthSummary {
            label: "Standby",
            tone: StatusTone::Standby,
        };
    }
    if accounts
        .iter()
        .any(|a| matches!(a.status, AccountStatus::Error | AccountStatus::NeedsReauth))
    {
        return HealthSummary {
            label: "Attention",
            tone: StatusTone::Warn,
        };
    }
    if accounts
        .iter()
        .any(|a| a.status == AccountStatus::Connected && a.enabled)
    {
        return HealthSummary {
            label: "Sync Active",
            tone: StatusTone::Connected,
        };
    }
    HealthSummary {
        label: "All Paused",
        tone: StatusTone::Paused,
    }
}

pub fn format_platform_label(platform: &str) -> &'static str {
    match platform {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        _ => "Desktop",
    }
}

pub fn format_modified_date(modified_ms: i64) -> String {
    if modified_ms <= 0 {
        return "\u{2014}".into();
    }
    let total_secs = modified_ms / 1000;
    let days = total_secs.div_euclid(86_400);
    let secs_of_day = total_secs.rem_euclid(86_400);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    format!("{year:04}-{m:02}-{d:02} {hour:02}:{minute:02}")
}
