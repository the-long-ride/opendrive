//! Path resolution, name sanitization, and RFC3339 timestamp parsing helpers.

use crate::engine::SyncEngine;
use crate::NodeId;
use std::collections::HashSet;
use std::path::PathBuf;

impl SyncEngine {
    pub fn node_rel_path(&self, root_node: NodeId, node_id: NodeId) -> Option<PathBuf> {
        if node_id == root_node {
            return Some(PathBuf::new());
        }
        let store = self.store.lock();
        let mut cur = node_id as i64;
        let mut seen = HashSet::new();
        let mut parts = Vec::new();
        while cur != root_node as i64 {
            if !seen.insert(cur) || cur <= 0 {
                return None;
            }
            let node = store.get_node(cur).ok().flatten()?;
            parts.push(node.effective_name().to_string());
            cur = node.parent_id?;
        }
        parts.reverse();
        let mut out = PathBuf::new();
        for p in parts {
            out.push(p);
        }
        Some(out)
    }

    pub fn resolve_remote_path(&self, root_node: NodeId, rel_path: &str) -> Option<NodeId> {
        let store = self.store.lock();
        store.get_node(root_node as i64).ok().flatten()?;
        let mut cur = root_node as i64;
        for raw in rel_path.split(['/', '\\']) {
            if raw.is_empty() || raw == "." {
                continue;
            }
            if raw == ".." {
                return None;
            }
            let children = store.list_children(cur).ok()?;
            let exact = children
                .iter()
                .find(|c| c.display_name == raw || c.name == raw);
            let matched = exact.or_else(|| {
                children.iter().find(|c| {
                    c.display_name.eq_ignore_ascii_case(raw) || c.name.eq_ignore_ascii_case(raw)
                })
            })?;
            cur = matched.id;
        }
        Some(cur as NodeId)
    }

    pub fn placeholder_folder_rel_paths(&self, root_node: NodeId) -> Vec<PathBuf> {
        let store = self.store.lock();
        let mut out = Vec::new();
        let mut stack = vec![(root_node as i64, PathBuf::new())];
        while let Some((pid, rel)) = stack.pop() {
            let Ok(children) = store.list_children(pid) else {
                continue;
            };
            for child in children {
                if child.kind != "folder" {
                    continue;
                }
                let child_rel = rel.join(&child.display_name);
                let has_grand = store
                    .list_children(child.id)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                if child.state == opendrive_store::NodeState::Placeholder && !has_grand {
                    out.push(child_rel);
                } else {
                    stack.push((child.id, child_rel));
                }
            }
        }
        out
    }
}

pub(crate) fn sanitize_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    if clean.is_empty() {
        "file".into()
    } else {
        clean
    }
}

pub(crate) fn parse_rfc3339_ms(s: &str) -> i64 {
    let b = s.trim().as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return 0;
    }
    if !matches!(b[10], b'T' | b't' | b' ') {
        return 0;
    }
    let Some(year) = std::str::from_utf8(&b[0..4])
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
    else {
        return 0;
    };
    let (Some(month), Some(day), Some(hour), Some(min), Some(sec)) = (
        parse_u2(&b[5..7]),
        parse_u2(&b[8..10]),
        parse_u2(&b[11..13]),
        parse_u2(&b[14..16]),
        parse_u2(&b[17..19]),
    ) else {
        return 0;
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 60 {
        return 0;
    }
    let mut idx = 19;
    let mut millis = 0i64;
    if idx < b.len() && b[idx] == b'.' {
        idx += 1;
        let frac_start = idx;
        while idx < b.len() && b[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx == frac_start {
            return 0;
        }
        let mut mul = 100i64;
        for &ch in &b[frac_start..idx.min(frac_start + 3)] {
            millis += (ch - b'0') as i64 * mul;
            mul /= 10;
        }
    }
    let tz_secs = match b.get(idx) {
        Some(b'Z' | b'z') if idx + 1 == b.len() => 0i64,
        Some(sign @ (b'+' | b'-')) if idx + 6 == b.len() && b[idx + 3] == b':' => {
            let (Some(th), Some(tm)) = (
                parse_u2(&b[idx + 1..idx + 3]),
                parse_u2(&b[idx + 4..idx + 6]),
            ) else {
                return 0;
            };
            if th > 23 || tm > 59 {
                return 0;
            }
            let secs = th * 3600 + tm * 60;
            if *sign == b'+' {
                secs
            } else {
                -secs
            }
        }
        _ => return 0,
    };
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86_400 + hour * 3600 + min * 60 + sec - tz_secs;
    secs * 1000 + millis
}

fn parse_u2(b: &[u8]) -> Option<i64> {
    if b.len() != 2 || !b[0].is_ascii_digit() || !b[1].is_ascii_digit() {
        return None;
    }
    Some(((b[0] - b'0') * 10 + (b[1] - b'0')) as i64)
}

#[cfg(test)]
mod tests {
    use super::parse_rfc3339_ms;

    #[test]
    fn parses_rfc3339_variants_and_rejects_invalid() {
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), 0);
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:01.25Z"), 1250);
        assert_eq!(parse_rfc3339_ms("1970-01-01T02:00:00+02:00"), 0);
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00-01:00"), 3_600_000);
        assert_eq!(
            parse_rfc3339_ms("2024-02-29T12:00:00.500Z"),
            1_709_208_000_500
        );
        assert_eq!(parse_rfc3339_ms("invalid"), 0);
        assert_eq!(parse_rfc3339_ms("2024-13-01T00:00:00Z"), 0);
        assert_eq!(super::sanitize_name(""), "file");
        assert_eq!(
            super::sanitize_name("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        for bad in [
            "short",
            "1970-01-01X00:00:00Z",
            "abcd-01-01T00:00:00Z",
            "1970-ab-01T00:00:00Z",
            "1970-01-01T00:00:00.Z",
            "1970-01-01T00:00:00+ab:00",
            "1970-01-01T00:00:00+25:00",
            "1970-01-01T00:00:00Q",
        ] {
            assert_eq!(parse_rfc3339_ms(bad), 0, "input={bad}");
        }
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00-02:00"), 7_200_000);
    }
}
