//! Optional GitHub release checker (off by default) with semver comparison.

use eframe::egui;
use serde::Deserialize;
use std::sync::mpsc;

pub const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/the-long-ride/opendrive/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub name: String,
    pub html_url: String,
    pub body: String,
}

#[derive(Deserialize)]
struct GithubReleasePayload {
    tag_name: String,
    #[serde(default)]
    name: String,
    html_url: String,
    #[serde(default)]
    body: String,
}

pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    let core = trimmed.split(['-', '+']).next()?.trim();
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn is_newer_release(current: &str, candidate: &str) -> bool {
    match (parse_semver(current), parse_semver(candidate)) {
        (Some(cur), Some(cand)) => cand > cur,
        _ => false,
    }
}

pub fn fetch_latest_release(
    url: &str,
    current_version: &str,
) -> Result<Option<ReleaseInfo>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .get(url)
            .header("User-Agent", format!("OpenDrive/{current_version}"))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("GitHub API returned HTTP {}", resp.status()));
        }
        let payload: GithubReleasePayload = resp.json().await.map_err(|e| e.to_string())?;
        if !is_newer_release(current_version, &payload.tag_name) {
            return Ok(None);
        }
        Ok(Some(ReleaseInfo {
            tag_name: payload.tag_name,
            name: payload.name,
            html_url: payload.html_url,
            body: payload.body,
        }))
    })
}

pub struct ReleaseCheckHandle {
    pub manual: bool,
    pub rx: mpsc::Receiver<Result<Option<ReleaseInfo>, String>>,
}

pub fn spawn_release_check(
    url: String,
    current_version: String,
    manual: bool,
    repaint: Option<egui::Context>,
) -> ReleaseCheckHandle {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let res = fetch_latest_release(&url, &current_version);
        let _ = tx.send(res);
        if let Some(ctx) = repaint {
            ctx.request_repaint();
        }
    });
    ReleaseCheckHandle { manual, rx }
}
