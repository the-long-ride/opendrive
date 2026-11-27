//! In-app log hub and tracing subscriber layer for Terminal Log & structured App Logs tabs.

pub use crate::runtime::log_mask::mask_sensitive_text;
use opendrive_core::{infer_diagnostic_code, LogSeverity};
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Mutex, Once, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

const MAX_LOG_ENTRIES: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppLogEntry {
    pub seq: u64,
    pub timestamp: String,
    pub level: LogSeverity,
    pub target: String,
    pub code: Option<String>,
    pub message: String,
    pub terminal_line: String,
}

pub struct AppLogHub {
    entries: Mutex<VecDeque<AppLogEntry>>,
    next_seq: Mutex<u64>,
}

static GLOBAL_HUB: OnceLock<AppLogHub> = OnceLock::new();
static LOG_INIT: Once = Once::new();

pub fn app_log_hub() -> &'static AppLogHub {
    GLOBAL_HUB.get_or_init(AppLogHub::new)
}

impl Default for AppLogHub {
    fn default() -> Self {
        Self::new()
    }
}

impl AppLogHub {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_LOG_ENTRIES)),
            next_seq: Mutex::new(1),
        }
    }

    pub fn record_event(
        &self,
        level: LogSeverity,
        target: &str,
        code: Option<&str>,
        message: &str,
    ) {
        let trimmed = message.trim();
        let masked_msg = mask_sensitive_text(trimmed);
        if masked_msg.is_empty() {
            return;
        }
        let resolved_code = code
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_string)
            .or_else(|| infer_diagnostic_code(level, target, trimmed).map(str::to_string));
        let timestamp = utc_timestamp_now();
        let code_tag = resolved_code
            .as_deref()
            .map_or_else(String::new, |c| format!("[{c}] "));
        let terminal_line = format!(
            "{timestamp} {:>5} {target}: {code_tag}{masked_msg}",
            level.as_str()
        );
        let seq = {
            let mut n = self.next_seq.lock().expect("log seq lock");
            let cur = *n;
            *n += 1;
            cur
        };
        let entry = AppLogEntry {
            seq,
            timestamp,
            level,
            target: target.to_string(),
            code: resolved_code,
            message: masked_msg,
            terminal_line,
        };
        let mut guard = self.entries.lock().expect("log hub lock");
        if guard.len() >= MAX_LOG_ENTRIES {
            guard.pop_front();
        }
        guard.push_back(entry);
    }

    pub fn snapshot(&self) -> Vec<AppLogEntry> {
        self.entries
            .lock()
            .expect("log hub lock")
            .iter()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.entries.lock().expect("log hub lock").clear();
    }
}

pub fn format_terminal_text(entries: &[AppLogEntry]) -> String {
    entries
        .iter()
        .map(|e| e.terminal_line.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_app_logs_text(entries: &[AppLogEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            let code = e.code.as_deref().unwrap_or("-");
            format!(
                "[{}] {} [{}] ({}) {}",
                e.timestamp, e.level, code, e.target, e.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn utc_timestamp_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

const fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (y + if m <= 2 { 1 } else { 0 }, m as u32, d as u32)
}

#[derive(Default)]
struct EventFieldVisitor {
    message: String,
    code: Option<String>,
    extras: String,
}

impl Visit for EventFieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message = value.to_string(),
            "code" => self.code = Some(value.to_string()),
            other => {
                let _ = write!(self.extras, " {other}={value}");
            }
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => self.message = format!("{value:?}"),
            "code" => self.code = Some(format!("{value:?}").trim_matches('"').to_string()),
            other => {
                let _ = write!(self.extras, " {other}={value:?}");
            }
        }
    }
}

pub struct AppLogTracingLayer {
    hub: &'static AppLogHub,
}

impl AppLogTracingLayer {
    pub fn with_hub(hub: &'static AppLogHub) -> Self {
        Self { hub }
    }
}

impl<S: Subscriber> Layer<S> for AppLogTracingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let target = meta.target();
        let severity = match *meta.level() {
            Level::ERROR => LogSeverity::Error,
            Level::WARN => LogSeverity::Warn,
            Level::INFO if target.starts_with("opendrive") => LogSeverity::Info,
            _ => return,
        };
        let mut visitor = EventFieldVisitor::default();
        event.record(&mut visitor);
        let full_msg = if visitor.message.is_empty() {
            visitor.extras.trim().to_string()
        } else {
            format!("{}{}", visitor.message, visitor.extras)
        };
        self.hub
            .record_event(severity, target, visitor.code.as_deref(), &full_msg);
    }
}

struct MaskedStderrWriter(Vec<u8>);

impl std::io::Write for MaskedStderrWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.0.is_empty() {
            return Ok(());
        }
        let raw = String::from_utf8_lossy(&self.0);
        let masked = mask_sensitive_text(&raw);
        let res = std::io::Write::write_all(&mut std::io::stderr().lock(), masked.as_bytes());
        self.0.clear();
        res
    }
}

impl Drop for MaskedStderrWriter {
    fn drop(&mut self) {
        let _ = std::io::Write::flush(self);
    }
}

pub fn init_app_logging() {
    LOG_INIT.call_once(|| {
        let filter = tracing_subscriber::filter::filter_fn(|meta| {
            *meta.level() <= Level::WARN || meta.target().starts_with("opendrive")
        });
        let registry = tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(|| MaskedStderrWriter(Vec::new()))
                    .with_filter(filter),
            )
            .with(AppLogTracingLayer::with_hub(app_log_hub()));
        let _ = tracing::subscriber::set_global_default(registry);
    });
}
