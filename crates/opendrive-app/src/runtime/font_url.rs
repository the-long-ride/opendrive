//! Public HTTPS CDN font URL validation and binary format helpers.

use reqwest::Url;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn is_url_like_input(raw: &str) -> bool {
    let t = raw.trim();
    t.contains("://")
        || (t.len() >= 5 && t[..5].eq_ignore_ascii_case("http:"))
        || (t.len() >= 6 && t[..6].eq_ignore_ascii_case("https:"))
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    let is_cgnat = a == 100 && (64..=127).contains(&b);
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || is_cgnat)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_ipv4(v4);
    }
    let seg0 = ip.segments()[0];
    let is_unique_local = (seg0 & 0xfe00) == 0xfc00;
    let is_link_local = (seg0 & 0xffc0) == 0xfe80;
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || is_unique_local
        || is_link_local)
}

pub fn validate_public_font_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|e| format!("Invalid font URL: {e}"))?;
    if url.scheme() != "https" {
        return Err("Font URL must use a public https:// link.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Font URL must be a public link without credentials.".into());
    }
    let raw_host = url
        .host_str()
        .ok_or_else(|| "Font URL is missing a host.".to_string())?;
    let unbracketed = raw_host.trim_start_matches('[').trim_end_matches(']');
    let host_ok = match unbracketed.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => is_public_ipv4(ip),
        Ok(IpAddr::V6(ip)) => is_public_ipv6(ip),
        Err(_) => {
            let d = raw_host.trim_end_matches('.').to_ascii_lowercase();
            let private_suffix = [".localhost", ".local", ".internal", ".lan", ".home.arpa"]
                .iter()
                .any(|sfx| d.ends_with(sfx));
            !d.is_empty() && d != "localhost" && d.contains('.') && !private_suffix
        }
    };
    if !host_ok {
        return Err(
            "Font URL must point to a public internet host (local/private addresses are blocked)."
                .into(),
        );
    }
    Ok(url)
}

pub fn detect_font_extension(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"OTTO") {
        "otf"
    } else if bytes.starts_with(b"ttcf") {
        "ttc"
    } else {
        "ttf"
    }
}
