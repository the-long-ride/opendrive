//! Tests for diagnostic error/warning code catalog, log credential masking, AppLogHub, and tracing layer.

use opendrive_app::desktop::terminal::filter_log_entries;
use opendrive_app::desktop::{LogFilterLevel, TerminalTab};
use opendrive_app::runtime::app_logs::{
    format_app_logs_text, format_terminal_text, init_app_logging, mask_sensitive_text, AppLogHub,
    AppLogTracingLayer,
};
use opendrive_core::{infer_diagnostic_code, lookup_diagnostic, LogSeverity, DIAGNOSTIC_CODES};
use tracing_subscriber::prelude::*;

#[test]
fn diagnostic_codes_masking_and_log_hub_work() {
    assert!(DIAGNOSTIC_CODES.len() >= 20);
    for d in DIAGNOSTIC_CODES {
        assert_eq!(lookup_diagnostic(d.code).map(|x| x.title), Some(d.title));
    }
    assert!(lookup_diagnostic("OD-NONE-999").is_none());
    assert_eq!(
        infer_diagnostic_code(LogSeverity::Info, "opendrive", "client_secret"),
        None
    );
    for (msg, expected) in [
        ("invalid_client", "OD-AUTH-001"),
        ("invalid_grant expired", "OD-AUTH-002"),
        ("browser redirect failed", "OD-AUTH-003"),
        ("service account json bad", "OD-AUTH-004"),
        ("keyring locked", "OD-AUTH-005"),
        ("needs reauth", "OD-AUTH-006"),
        ("403 insufficient scope", "OD-SYNC-002"),
        ("429 rate limit", "OD-SYNC-005"),
        ("conflict detected", "OD-SYNC-003"),
        ("this pc namespace failed", "OD-MNT-002"),
        ("cfapi mount error", "OD-MNT-001"),
        ("stream 206 error", "OD-STRM-001"),
        ("open with failed", "OD-STRM-002"),
        (".odcred decrypt error", "OD-CFG-002"),
        ("github api release error", "OD-CFG-003"),
        ("font decode failed", "OD-FONT-001"),
        ("sqlite cache error", "OD-SYNC-004"),
        ("something else", "OD-SYNC-001"),
    ] {
        assert_eq!(
            infer_diagnostic_code(LogSeverity::Error, "opendrive", msg),
            Some(expected)
        );
    }
    assert_eq!(
        infer_diagnostic_code(LogSeverity::Warn, "opendrive", "misc"),
        Some("OD-MNT-003")
    );

    let raw = "cid=123456-abc.apps.googleusercontent.com client_secret=GOCSPX-supersecret123 access_token=\"ya29.secret_tok\" refresh_token=1//ref_secret Authorization: Bearer abc.def.ghi password='MyPass!99' \"private_key\": \"-----BEGIN PRIVATE KEY-----\\nABC\\n-----END PRIVATE KEY-----\"";
    let masked = mask_sensitive_text(raw);
    let secrets = [
        "123456-abc",
        "supersecret123",
        "secret_tok",
        "ref_secret",
        "abc.def.ghi",
        "MyPass!99",
        "BEGIN PRIVATE KEY",
    ];
    for secret in secrets {
        assert!(!masked.contains(secret), "leaked {secret}: {masked}");
    }
    assert!(
        masked.contains("***.apps.googleusercontent.com")
            && masked.contains("[REDACTED PRIVATE KEY]")
    );

    let hub = AppLogHub::default();
    hub.record_event(LogSeverity::Info, "opendrive::auth", None, "   ");
    hub.record_event(
        LogSeverity::Info,
        "opendrive::auth",
        None,
        "token=secret123 ok",
    );
    hub.record_event(
        LogSeverity::Error,
        "opendrive::auth",
        Some("OD-AUTH-001"),
        "invalid_client GOCSPX-abc",
    );
    let snap = hub.snapshot();
    assert_eq!(snap.len(), 2);
    assert!(format_terminal_text(&snap).contains("OD-AUTH-001"));
    assert!(format_app_logs_text(&snap).contains("token=***"));
    hub.clear();
    assert!(hub.snapshot().is_empty());

    let static_hub = Box::leak(Box::new(AppLogHub::new()));
    let subscriber = tracing_subscriber::registry().with(AppLogTracingLayer::with_hub(static_hub));
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "opendrive::test", "connected cid=99-x.apps.googleusercontent.com");
        tracing::warn!(target: "opendrive::sync", code = "OD-SYNC-003", "conflict file");
        tracing::info!(target: "reqwest::wire", "ignored non-opendrive info");
    });
    let entries = static_hub.snapshot();
    assert_eq!(entries.len(), 2);

    let mut tv = opendrive_app::desktop::TerminalViewState {
        active_tab: TerminalTab::AppLogs,
        search_query: "OD-SYNC-003".into(),
        level_filter: LogFilterLevel::WarnPlus,
    };
    assert_eq!(filter_log_entries(&entries, &tv).len(), 1);
    tv.level_filter = LogFilterLevel::ErrorOnly;
    assert_eq!(filter_log_entries(&entries, &tv).len(), 0);

    // Ring-buffer eviction (>1000), record_debug extra fields, and global init_app_logging()
    for i in 0..1005 {
        hub.record_event(
            LogSeverity::Info,
            "opendrive::ring",
            None,
            &format!("msg {i}"),
        );
    }
    assert_eq!(hub.snapshot().len(), 1000);
    init_app_logging();
    tracing::error!(target: "opendrive::cov", count = 42, "structured debug field test");
    let sub2 = tracing_subscriber::registry().with(AppLogTracingLayer::with_hub(static_hub));
    tracing::subscriber::with_default(sub2, || {
        tracing::error!(target: "opendrive::cov", attempt = 3, "debug count failure");
    });
    assert!(static_hub
        .snapshot()
        .iter()
        .any(|e| e.message.contains("attempt=3")));

    let extra = mask_sensitive_text("BEARER tok99 code=403 code=oauth_secret \"token\":\"a\\\"b\"");
    assert!(
        extra.contains("Bearer ***")
            && extra.contains("code=403")
            && !extra.contains("oauth_secret")
    );
    hub.clear();
    hub.record_event(
        LogSeverity::Error,
        "opendrive::sync",
        None,
        "code=403 access_denied token=ya29.secret",
    );
    let inferred = hub.snapshot();
    assert_eq!(inferred[0].code.as_deref(), Some("OD-SYNC-002"));
    assert!(
        inferred[0].message.contains("code=403") && !inferred[0].message.contains("ya29.secret")
    );
}
