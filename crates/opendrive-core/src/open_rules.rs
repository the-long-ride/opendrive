//! Media kind classification and user-configured "Open with" rule matching.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Folder,
    Image,
    Video,
    Pdf,
    Audio,
    Archive,
    CodeText,
    NativeDoc,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenWithRule {
    pub pattern: String,
    pub app_command: String,
}

const KNOWN_HTTP_STREAM_PLAYERS: &[&str] = &[
    "vlc",
    "mpv",
    "iina",
    "iina-cli",
    "ffplay",
    "mpc-hc",
    "mpc-hc64",
    "mpc-be",
    "mpc-be64",
    "potplayer",
    "potplayermini",
    "potplayermini64",
    "smplayer",
    "celluloid",
    "totem",
    "quicktime",
    "quicktime player",
];

fn extension_lower(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty())
}

pub fn classify_media_kind(name: &str, mime: Option<&str>) -> MediaKind {
    if let Some(raw_mime) = mime {
        let m = raw_mime.trim().to_ascii_lowercase();
        if m == "application/vnd.google-apps.folder" {
            return MediaKind::Folder;
        }
        if m.starts_with("application/vnd.google-apps.")
            && m != "application/vnd.google-apps.shortcut"
        {
            return MediaKind::NativeDoc;
        }
        if m == "application/pdf" {
            return MediaKind::Pdf;
        }
        if m.starts_with("image/") {
            return MediaKind::Image;
        }
        if m.starts_with("video/") {
            return MediaKind::Video;
        }
        if m.starts_with("audio/") {
            return MediaKind::Audio;
        }
        if m.starts_with("text/")
            || matches!(
                m.as_str(),
                "application/typescript"
                    | "application/javascript"
                    | "application/json"
                    | "application/xml"
            )
        {
            return MediaKind::CodeText;
        }
    }

    let Some(ext) = extension_lower(name) else {
        return MediaKind::Other;
    };
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "heic" | "avif"
        | "tiff" | "tif" | "jfif" => MediaKind::Image,
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" | "ts" | "flv" | "wmv" | "3gp" | "ogv"
        | "mpg" | "mpeg" => MediaKind::Video,
        "pdf" => MediaKind::Pdf,
        "mp3" | "flac" | "wav" | "ogg" | "aac" | "m4a" | "opus" | "wma" | "aiff" | "mid"
        | "midi" => MediaKind::Audio,
        "zip" | "tar" | "gz" | "7z" | "rar" | "bz2" | "xz" | "tgz" | "zst" => MediaKind::Archive,
        "rs" | "js" | "py" | "json" | "md" | "txt" | "yaml" | "toml" | "xml" | "csv" | "html"
        | "htm" | "css" | "scss" | "tsx" | "jsx" | "mjs" | "cjs" | "sh" | "sql" | "c" | "cpp"
        | "h" | "hpp" | "go" | "java" | "kt" | "cs" | "rb" | "php" | "yml" | "log" | "ini"
        | "conf" => MediaKind::CodeText,
        _ => MediaKind::Other,
    }
}

fn group_kind(group: &str) -> Option<MediaKind> {
    match group {
        "@video" => Some(MediaKind::Video),
        "@image" => Some(MediaKind::Image),
        "@pdf" => Some(MediaKind::Pdf),
        "@audio" => Some(MediaKind::Audio),
        "@code" => Some(MediaKind::CodeText),
        "@archive" => Some(MediaKind::Archive),
        _ => None,
    }
}

fn rule_matches(pattern: &str, kind: MediaKind, ext: Option<&str>) -> bool {
    for raw_token in pattern.split(|c: char| c == ',' || c.is_ascii_whitespace()) {
        let token = raw_token.trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }
        let matched = if token == "*" {
            true
        } else if let Some(gk) = group_kind(&token) {
            kind == gk
        } else {
            let pat_ext = token
                .strip_prefix("*.")
                .or_else(|| token.strip_prefix('.'))
                .unwrap_or(&token);
            !pat_ext.is_empty() && kind != MediaKind::Folder && ext.is_some_and(|e| e == pat_ext)
        };
        if matched {
            return true;
        }
    }
    false
}

pub fn resolve_open_app<'a>(
    rules: &'a [OpenWithRule],
    name: &str,
    mime: Option<&str>,
) -> Option<&'a str> {
    let kind = classify_media_kind(name, mime);
    let ext = extension_lower(name);
    for rule in rules {
        let cmd = rule.app_command.trim();
        if cmd.is_empty() {
            continue;
        }
        if rule_matches(&rule.pattern, kind, ext.as_deref()) {
            return Some(cmd);
        }
    }
    None
}

fn extract_executable_token(cmd: &str) -> Option<&str> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(quote) = trimmed.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let rest = &trimmed[quote.len_utf8()..];
        return rest.split(quote).next().filter(|s| !s.is_empty());
    }
    trimmed.split_whitespace().next()
}

fn normalize_single_token(raw_token: &str) -> Result<String, String> {
    let token = raw_token.trim().to_ascii_lowercase();
    if token == "*" {
        return Ok(token);
    }
    if token.starts_with('@') {
        if group_kind(&token).is_some() {
            return Ok(token);
        }
        return Err(format!(
            "Unknown media group '{raw_token}'. Valid groups: @video, @image, @pdf, @audio, @code, @archive."
        ));
    }
    let ext = token
        .strip_prefix("*.")
        .or_else(|| token.strip_prefix('.'))
        .unwrap_or(&token);
    if !ext.is_empty()
        && ext
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+'))
    {
        Ok(ext.to_string())
    } else {
        Err(format!(
            "Invalid file extension '{raw_token}'. Use extensions like mp4, mkv or groups like @video."
        ))
    }
}

/// Normalizes and validates comma/whitespace-separated extensions, `@group` names, or `*`.
pub fn normalize_rule_tokens(pattern: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for raw in pattern.split(|c: char| c == ',' || c.is_ascii_whitespace()) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let canon = normalize_single_token(trimmed)?;
        if !out.contains(&canon) {
            out.push(canon);
        }
    }
    if out.is_empty() {
        return Err("Enter at least one file extension or @group.".into());
    }
    Ok(out)
}

fn has_balanced_quotes(raw: &str) -> bool {
    let mut active = None::<char>;
    for ch in raw.chars() {
        match active {
            Some(q) if ch == q => active = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => active = Some(ch),
            None => {}
        }
    }
    active.is_none()
}

fn group_covers_ext(group: &str, ext: &str) -> bool {
    !ext.starts_with('@')
        && ext != "*"
        && group_kind(group)
            .is_some_and(|gk| classify_media_kind(&format!("file.{ext}"), None) == gk)
}

fn tokens_conflict(a: &str, b: &str) -> bool {
    a == b || a == "*" || b == "*" || group_covers_ext(a, b) || group_covers_ext(b, a)
}

/// Validates and canonicalizes an Open With rule and ensures no extension/group conflicts across apps.
pub fn validate_open_with_rule(
    pattern: &str,
    app_command: &str,
    existing: &[OpenWithRule],
    ignore_index: Option<usize>,
) -> Result<OpenWithRule, String> {
    let tokens = normalize_rule_tokens(pattern)?;
    let cmd = app_command.trim();
    if cmd.is_empty() {
        return Err("Enter an application command or executable path.".into());
    }
    if !has_balanced_quotes(cmd) {
        return Err("Unclosed quote in application command.".into());
    }
    for (idx, other) in existing.iter().enumerate() {
        if Some(idx) == ignore_index {
            continue;
        }
        let Ok(other_tokens) = normalize_rule_tokens(&other.pattern) else {
            continue;
        };
        if let Some(conflict) = tokens
            .iter()
            .find(|t| other_tokens.iter().any(|o| tokens_conflict(t, o)))
        {
            return Err(format!(
                "'{conflict}' conflicts with rule '{}' ({}). Each extension can only open with one app.",
                other.pattern.trim(),
                other.app_command.trim()
            ));
        }
    }
    Ok(OpenWithRule {
        pattern: tokens.join(", "),
        app_command: cmd.to_string(),
    })
}

pub fn supports_http_stream_arg(app_command: Option<&str>) -> bool {
    let Some(exe) = app_command.and_then(extract_executable_token) else {
        return false;
    };
    let basename = exe.rsplit(['/', '\\']).next().unwrap_or(exe).trim();
    let lower = basename.to_ascii_lowercase();
    let stem = [".exe", ".app", ".cmd", ".bat"]
        .iter()
        .find_map(|s| lower.strip_suffix(s))
        .unwrap_or(&lower);
    KNOWN_HTTP_STREAM_PLAYERS.contains(&stem)
}
