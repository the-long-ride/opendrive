//! Local font file and Google Fonts family validation, download, and background import worker.

use crate::runtime::font_url::{
    detect_font_extension, is_url_like_input, validate_public_font_url,
};
use crate::runtime::fonts::validate_font_bytes;
use eframe::egui;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

pub const GOOGLE_FONTS_CSS_BASE: &str = "https://fonts.googleapis.com/css2";
const MAX_FONT_BYTES: usize = 20 * 1024 * 1024;

pub fn encode_google_font_query(family: &str) -> String {
    let words: Vec<String> = family
        .split_whitespace()
        .map(|part| {
            let mut out = String::new();
            for b in part.as_bytes() {
                if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                    out.push(*b as char);
                } else {
                    let _ = write!(out, "%{b:02X}");
                }
            }
            out
        })
        .collect();
    words.join("+")
}

pub fn extract_ttf_url_from_css(css: &str) -> Option<String> {
    let mut rest = css;
    while let Some(start) = rest.find("url(") {
        let after = &rest[start + 4..];
        let Some(end) = after.find(')') else {
            break;
        };
        let raw = after[..end].trim().trim_matches(['\'', '"']);
        if (raw.starts_with("https://") || raw.starts_with("http://"))
            && (raw.contains(".ttf") || raw.contains(".otf"))
        {
            return Some(raw.to_string());
        }
        rest = &after[end + 1..];
    }
    None
}

fn sanitize_font_filename(family: &str, ext: &str) -> String {
    let stem: String = family
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let clean = stem.trim_matches('_');
    let base = if clean.is_empty() {
        "custom_font"
    } else {
        clean
    };
    format!("{base}.{ext}")
}

pub fn import_local_font_file(path: &Path, fonts_dir: &Path) -> Result<(String, PathBuf), String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("Failed to read font file {}: {e}", path.display()))?;
    let parsed_family = validate_font_bytes(&bytes)?;
    let family = parsed_family.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Custom Font")
            .to_string()
    });
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| matches!(e.as_str(), "ttf" | "otf" | "ttc"))
        .unwrap_or_else(|| "ttf".to_string());
    std::fs::create_dir_all(fonts_dir)
        .map_err(|e| format!("Failed to create fonts directory: {e}"))?;
    let dest = fonts_dir.join(sanitize_font_filename(&family, &ext));
    std::fs::write(&dest, &bytes).map_err(|e| format!("Failed to save font file: {e}"))?;
    Ok((family, dest))
}

async fn fetch_font_response(
    client: &reqwest::Client,
    url: &str,
) -> Result<(reqwest::StatusCode, String, String, Vec<u8>), String> {
    let resp = client
        .get(url)
        .header(
            "User-Agent",
            concat!("OpenDrive/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .map_err(|e| format!("Failed to reach font URL: {e}"))?;
    let status = resp.status();
    let final_url = resp.url().to_string();
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if resp
        .content_length()
        .is_some_and(|n| n as usize > MAX_FONT_BYTES)
    {
        return Err("Font file exceeds 20 MiB download limit".into());
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    if bytes.len() > MAX_FONT_BYTES {
        return Err("Font file exceeds 20 MiB download limit".into());
    }
    Ok((status, final_url, ctype, bytes))
}

fn save_imported_font(
    bytes: &[u8],
    fallback_family: &str,
    fonts_dir: &Path,
) -> Result<(String, PathBuf), String> {
    let resolved_family =
        validate_font_bytes(bytes)?.unwrap_or_else(|| fallback_family.to_string());
    std::fs::create_dir_all(fonts_dir)
        .map_err(|e| format!("Failed to create fonts directory: {e}"))?;
    let dest = fonts_dir.join(sanitize_font_filename(
        &resolved_family,
        detect_font_extension(bytes),
    ));
    std::fs::write(&dest, bytes).map_err(|e| format!("Failed to save font: {e}"))?;
    Ok((resolved_family, dest))
}

fn build_font_client(require_public: bool) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(12));
    if require_public {
        builder = builder.redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("Too many redirects while downloading font.");
            }
            match validate_public_font_url(attempt.url().as_str()) {
                Ok(_) => attempt.follow(),
                Err(msg) => attempt.error(msg),
            }
        }));
    }
    builder.build().map_err(|e| e.to_string())
}

fn check_font_http(
    status: reqwest::StatusCode,
    context: &str,
    err_4xx: &str,
) -> Result<(), String> {
    if matches!(status.as_u16(), 401 | 403) {
        return Err(format!(
            "{err_4xx} (HTTP {status}). Use a public https:// link."
        ));
    }
    if !status.is_success() {
        return Err(format!("{context} (HTTP {status})"));
    }
    Ok(())
}

pub fn import_google_font_family(
    family: &str,
    css_base: &str,
    fonts_dir: &Path,
) -> Result<(String, PathBuf), String> {
    let trimmed = family.trim();
    if trimmed.is_empty() {
        return Err(
            "Enter a font family name, public https:// font URL, or .ttf/.otf path.".into(),
        );
    }
    let css_url = format!(
        "{css_base}?family={}&display=swap",
        encode_google_font_query(trimmed)
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async {
        let client = build_font_client(false)?;
        let (st, _, _, css_bytes) = fetch_font_response(&client, &css_url).await?;
        check_font_http(
            st,
            &format!("Font family \"{trimmed}\" was not found on Google Fonts"),
            "Google Fonts access was denied",
        )?;
        let css = String::from_utf8_lossy(&css_bytes);
        let font_url = extract_ttf_url_from_css(&css)
            .ok_or_else(|| format!("No downloadable TTF/OTF font found for \"{trimmed}\""))?;
        let (fst, _, _, fbytes) = fetch_font_response(&client, &font_url).await?;
        check_font_http(
            fst,
            "Font download returned",
            "Font download access was denied",
        )?;
        save_imported_font(&fbytes, trimmed, fonts_dir)
    })
}

pub fn import_font_from_url_with_policy(
    url_str: &str,
    fonts_dir: &Path,
    require_public: bool,
) -> Result<(String, PathBuf), String> {
    let parsed = if require_public {
        validate_public_font_url(url_str)?
    } else {
        reqwest::Url::parse(url_str.trim()).map_err(|e| format!("Invalid font URL: {e}"))?
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async {
        let client = build_font_client(require_public)?;
        let (st, final_url, ctype, mut data) =
            fetch_font_response(&client, parsed.as_str()).await?;
        check_font_http(
            st,
            "Failed to download font from URL",
            "Font URL is not publicly accessible",
        )?;
        if require_public {
            validate_public_font_url(&final_url)?;
        }
        let is_css =
            ctype.contains("text/css") || String::from_utf8_lossy(&data).contains("@font-face");
        if is_css {
            let css = String::from_utf8_lossy(&data);
            let font_url = extract_ttf_url_from_css(&css)
                .ok_or_else(|| "CSS URL contains no downloadable .ttf/.otf font".to_string())?;
            if require_public {
                validate_public_font_url(&font_url)?;
            }
            let (fst, fend, _, fbytes) = fetch_font_response(&client, &font_url).await?;
            check_font_http(
                fst,
                "Font download returned",
                "Font URL is not publicly accessible",
            )?;
            if require_public {
                validate_public_font_url(&fend)?;
            }
            data = fbytes;
        }
        let fallback = parsed
            .path_segments()
            .and_then(|mut s| s.next_back())
            .and_then(|f| Path::new(f).file_stem().and_then(|s| s.to_str()))
            .filter(|s| !s.is_empty())
            .unwrap_or("CDN Font");
        save_imported_font(&data, fallback, fonts_dir)
    })
}

pub fn import_font_from_url(url_str: &str, fonts_dir: &Path) -> Result<(String, PathBuf), String> {
    import_font_from_url_with_policy(url_str, fonts_dir, true)
}

pub struct FontImportHandle {
    pub rx: mpsc::Receiver<Result<(String, PathBuf), String>>,
}

pub fn spawn_font_import(
    family_or_path: String,
    css_base: String,
    fonts_dir: PathBuf,
    repaint: Option<egui::Context>,
) -> FontImportHandle {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let trimmed = family_or_path.trim();
        let res = if is_url_like_input(trimmed) {
            import_font_from_url(trimmed, &fonts_dir)
        } else if Path::new(trimmed).is_file() {
            import_local_font_file(Path::new(trimmed), &fonts_dir)
        } else {
            import_google_font_family(trimmed, &css_base, &fonts_dir)
        };
        let _ = tx.send(res);
        if let Some(ctx) = repaint {
            ctx.request_repaint();
        }
    });
    FontImportHandle { rx }
}
