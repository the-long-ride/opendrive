//! Structured OpenDrive warning and error code catalog for logs, docs, and AI troubleshooting.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSeverity {
    Info,
    Warn,
    Error,
}

impl LogSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

impl fmt::Display for LogSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticCode {
    pub code: &'static str,
    pub severity: LogSeverity,
    pub category: &'static str,
    pub title: &'static str,
    pub guidance: &'static str,
}

pub const DIAGNOSTIC_CODES: &[DiagnosticCode] = &[
    DiagnosticCode {
        code: "OD-AUTH-001",
        severity: LogSeverity::Error,
        category: "Auth",
        title: "Invalid or rejected OAuth client credentials",
        guidance: "Verify Client ID/Secret in Google Cloud Console (Desktop app) and re-enter them via Re-auth -> Credentials.",
    },
    DiagnosticCode {
        code: "OD-AUTH-002",
        severity: LogSeverity::Error,
        category: "Auth",
        title: "OAuth token expired or revoked",
        guidance: "Click Sign In on the account row to refresh consent, or publish your OAuth consent screen beyond 7-day Testing.",
    },
    DiagnosticCode {
        code: "OD-AUTH-003",
        severity: LogSeverity::Warn,
        category: "Auth",
        title: "Browser launch or loopback redirect interrupted",
        guidance: "Use Copy Link to open the Google sign-in URL manually in your browser and ensure 127.0.0.1 is not blocked.",
    },
    DiagnosticCode {
        code: "OD-AUTH-004",
        severity: LogSeverity::Error,
        category: "Auth",
        title: "Service Account JSON or JWT exchange failed",
        guidance: "Validate the downloaded .json key file, system clock accuracy, and folder sharing with client_email.",
    },
    DiagnosticCode {
        code: "OD-AUTH-005",
        severity: LogSeverity::Error,
        category: "Auth",
        title: "OS keyring or secret store unavailable",
        guidance: "Ensure your OS credential manager / secret service is unlocked, or export/import via .odcred backup.",
    },
    DiagnosticCode {
        code: "OD-AUTH-006",
        severity: LogSeverity::Warn,
        category: "Auth",
        title: "Account requires re-authentication",
        guidance: "Use Sign In or Credentials on the account card to restore sync access.",
    },
    DiagnosticCode {
        code: "OD-SYNC-001",
        severity: LogSeverity::Error,
        category: "Sync",
        title: "Google Drive API request or sync failure",
        guidance: "Check network connectivity and retry the sync or refresh Explorer.",
    },
    DiagnosticCode {
        code: "OD-SYNC-002",
        severity: LogSeverity::Error,
        category: "Sync",
        title: "Insufficient Drive scope or permission (403)",
        guidance: "Enable Google Drive API, add .../auth/drive scope in Data Access, and re-authenticate.",
    },
    DiagnosticCode {
        code: "OD-SYNC-003",
        severity: LogSeverity::Warn,
        category: "Sync",
        title: "Sync conflict detected",
        guidance: "Inspect the conflict sibling file in the mounted folder or Explorer and keep the desired version.",
    },
    DiagnosticCode {
        code: "OD-SYNC-004",
        severity: LogSeverity::Error,
        category: "Sync",
        title: "Local replica or SQLite cache error",
        guidance: "Check disk space and permissions on the OpenDrive cache/data directory, or purge unpinned cache in Settings.",
    },
    DiagnosticCode {
        code: "OD-SYNC-005",
        severity: LogSeverity::Warn,
        category: "Sync",
        title: "Rate limited by Google Drive API (429)",
        guidance: "OpenDrive backs off automatically; wait briefly or lower concurrent Explorer actions.",
    },
    DiagnosticCode {
        code: "OD-MNT-001",
        severity: LogSeverity::Error,
        category: "Mount",
        title: "OS virtual filesystem mount failed",
        guidance: "Verify FUSE3 (Linux), loopback NFS (macOS), or Windows Cloud Files permissions and ensure no stale process locks the mount root.",
    },
    DiagnosticCode {
        code: "OD-MNT-002",
        severity: LogSeverity::Warn,
        category: "Mount",
        title: "Windows This PC namespace registration failed",
        guidance: "Toggle Show OpenDrive in This PC off and on in Settings or check HKCU registry access.",
    },
    DiagnosticCode {
        code: "OD-MNT-003",
        severity: LogSeverity::Warn,
        category: "Mount",
        title: "Remote mount sync interrupted",
        guidance: "Use Refresh or verify account status on the Connected Accounts dashboard.",
    },
    DiagnosticCode {
        code: "OD-STRM-001",
        severity: LogSeverity::Error,
        category: "Stream",
        title: "Localhost 206 media stream server error",
        guidance: "Retry opening the video or verify the account is connected and online.",
    },
    DiagnosticCode {
        code: "OD-STRM-002",
        severity: LogSeverity::Warn,
        category: "Stream",
        title: "Custom Open-With launcher failed",
        guidance: "Check the configured executable path or command under Settings -> Open With Rules.",
    },
    DiagnosticCode {
        code: "OD-CFG-001",
        severity: LogSeverity::Error,
        category: "Config",
        title: "Settings or account configuration persistence error",
        guidance: "Ensure the OpenDrive config directory is writable.",
    },
    DiagnosticCode {
        code: "OD-CFG-002",
        severity: LogSeverity::Error,
        category: "Config",
        title: "Encrypted .odcred vault backup/restore failed",
        guidance: "Verify your vault password (9+ chars with upper, lower, digit, symbol) and file permissions.",
    },
    DiagnosticCode {
        code: "OD-CFG-003",
        severity: LogSeverity::Warn,
        category: "Config",
        title: "GitHub release check failed",
        guidance: "Verify internet access to api.github.com and retry Check Now in Settings.",
    },
    DiagnosticCode {
        code: "OD-FONT-001",
        severity: LogSeverity::Error,
        category: "Font",
        title: "Invalid or unsupported UI font file or family",
        guidance: "Choose an installed system font, a valid Google Font family name (e.g. Inter), or a valid .ttf/.otf/.ttc file.",
    },
    DiagnosticCode {
        code: "OD-FONT-002",
        severity: LogSeverity::Warn,
        category: "Font",
        title: "Font download or persistence failed",
        guidance: "Check internet access to fonts.googleapis.com or browse for a local font file instead.",
    },
];

pub fn lookup_diagnostic(code: &str) -> Option<&'static DiagnosticCode> {
    DIAGNOSTIC_CODES.iter().find(|d| d.code == code)
}

pub fn infer_diagnostic_code(
    severity: LogSeverity,
    target: &str,
    message: &str,
) -> Option<&'static str> {
    if severity == LogSeverity::Info {
        return None;
    }
    let m = message.to_ascii_lowercase();
    let t = target.to_ascii_lowercase();
    if m.contains("invalid_client")
        || m.contains("unauthorized_client")
        || m.contains("client secret")
    {
        return Some("OD-AUTH-001");
    }
    if m.contains("invalid_grant") || m.contains("token expired") || m.contains("revoked") {
        return Some("OD-AUTH-002");
    }
    if m.contains("browser") || m.contains("loopback") || m.contains("redirect") {
        return Some("OD-AUTH-003");
    }
    if m.contains("service account") || m.contains("jwt") {
        return Some("OD-AUTH-004");
    }
    if m.contains("keyring") || m.contains("secret store") {
        return Some("OD-AUTH-005");
    }
    if m.contains("re-auth") || m.contains("reauth") {
        return Some("OD-AUTH-006");
    }
    if m.contains("403") || m.contains("insufficient") || m.contains("access_denied") {
        return Some("OD-SYNC-002");
    }
    if m.contains("429") || m.contains("rate limit") {
        return Some("OD-SYNC-005");
    }
    if m.contains("conflict") {
        return Some("OD-SYNC-003");
    }
    if m.contains("this pc") || m.contains("namespace") {
        return Some("OD-MNT-002");
    }
    if m.contains("mount") || m.contains("cfapi") || t.contains("fs_") {
        return Some("OD-MNT-001");
    }
    if m.contains("stream") || m.contains("206") {
        return Some("OD-STRM-001");
    }
    if m.contains("open with") || m.contains("launcher") {
        return Some("OD-STRM-002");
    }
    if m.contains(".odcred") || m.contains("vault") {
        return Some("OD-CFG-002");
    }
    if m.contains("release") || m.contains("github api") {
        return Some("OD-CFG-003");
    }
    if m.contains("font") {
        return Some("OD-FONT-001");
    }
    if m.contains("cache") || m.contains("sqlite") || m.contains("replica") {
        return Some("OD-SYNC-004");
    }
    Some(match severity {
        LogSeverity::Warn => "OD-MNT-003",
        LogSeverity::Error | LogSeverity::Info => "OD-SYNC-001",
    })
}
