//! 来源解析引擎模块
//!
//! 从 Grok 搜索响应中提取引用来源信息，支持 4 种解析策略。

#![allow(dead_code)]

use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::LazyLock;

static MD_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[([^\]]+)\]\((https?://[^)]+)\)").expect("markdown 链接正则必须有效")
});

static SOURCES_HEADING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?im)^(?:#{1,6}\s*)?(?:\*\*|__)?\s*(sources?|references?|citations?|信源|参考资料|参考|引用|来源列表|来源)\s*(?:\*\*|__)?(?:\s*[（(][^)\n]*[)）])?\s*[:：]?\s*$",
    )
    .expect("来源标题正则必须有效")
});

static SOURCES_FUNCTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?im)(^|\n)\s*(sources|source|citations|citation|references|reference|citation_card|source_cards|source_card)\s*\(",
    )
    .expect("来源函数调用正则必须有效")
});

static DETAILS_OPEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<details\b").expect("details 起始标签正则必须有效"));

static DETAILS_CLOSE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</details>").expect("details 结束标签正则必须有效"));

static LIST_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:[-*]|\d+\.)\s*").expect("列表前缀正则必须有效"));

static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"https?://[^\s<>"'`，。、；：！？》）】\)]+"#).expect("URL 正则必须有效")
});

static SINGLE_QUOTED_STRING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"'([^'\\]*(?:\\.[^'\\]*)*)'").expect("单引号字符串正则必须有效"));

/// 搜索结果来源
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Source {
    pub title: String,
    pub url: String,
}

pub fn parse_sources(text: &str) -> Vec<Source> {
    let (_, sources) = split_answer_and_sources(text);
    dedup_sources(sources)
}

pub fn strip_sources(text: &str) -> String {
    let (answer, _) = split_answer_and_sources(text);
    answer
}

pub fn extract_unique_urls(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut urls = Vec::new();

    for m in URL_RE.find_iter(text) {
        let url = trim_url_tail_punctuation(m.as_str());
        if url.is_empty() {
            continue;
        }
        if seen.insert(url.to_string()) {
            urls.push(url.to_string());
        }
    }

    urls
}

fn split_answer_and_sources(text: &str) -> (String, Vec<Source>) {
    let raw = text.trim();
    if raw.is_empty() {
        return (String::new(), Vec::new());
    }

    let strategies: [fn(&str) -> Option<(String, Vec<Source>)>; 4] = [
        split_function_call_sources,
        split_heading_sources,
        split_details_block_sources,
        split_tail_link_block,
    ];

    for strategy in strategies {
        if let Some((answer, sources)) = strategy(raw) {
            if !sources.is_empty() {
                return (answer, sources);
            }
        }
    }

    (raw.to_string(), Vec::new())
}

fn split_function_call_sources(text: &str) -> Option<(String, Vec<Source>)> {
    let matches: Vec<_> = SOURCES_FUNCTION_RE.find_iter(text).collect();
    if matches.is_empty() {
        return None;
    }

    for m in matches.iter().rev() {
        let open_paren_idx = m.end().saturating_sub(1);
        let args_text = match extract_balanced_call_at_end(text, open_paren_idx) {
            Some(args) => args,
            None => continue,
        };

        let sources = parse_sources_payload(&args_text);
        if sources.is_empty() {
            continue;
        }

        let answer = text[..m.start()].trim_end().to_string();
        return Some((answer, sources));
    }

    None
}

fn extract_balanced_call_at_end(text: &str, open_paren_idx: usize) -> Option<String> {
    let bytes = text.as_bytes();
    if open_paren_idx >= bytes.len() || bytes[open_paren_idx] != b'(' {
        return None;
    }

    let mut depth = 1usize;
    let mut in_string: Option<u8> = None;
    let mut escape = false;

    for idx in (open_paren_idx + 1)..bytes.len() {
        let ch = bytes[idx];

        if let Some(quote) = in_string {
            if escape {
                escape = false;
                continue;
            }
            if ch == b'\\' {
                escape = true;
                continue;
            }
            if ch == quote {
                in_string = None;
            }
            continue;
        }

        if ch == b'\'' || ch == b'"' {
            in_string = Some(ch);
            continue;
        }

        if ch == b'(' {
            depth += 1;
            continue;
        }

        if ch == b')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                if !text[(idx + 1)..].trim().is_empty() {
                    return None;
                }
                return Some(text[(open_paren_idx + 1)..idx].to_string());
            }
        }
    }

    None
}

fn split_heading_sources(text: &str) -> Option<(String, Vec<Source>)> {
    let matches: Vec<_> = SOURCES_HEADING_RE.find_iter(text).collect();
    if matches.is_empty() {
        return None;
    }

    for m in matches.iter().rev() {
        let start = m.start();
        let sources_text = &text[start..];
        let sources = extract_sources_from_text(sources_text);
        if sources.is_empty() {
            continue;
        }

        let answer = text[..start].trim_end().to_string();
        return Some((answer, sources));
    }

    None
}

fn split_details_block_sources(text: &str) -> Option<(String, Vec<Source>)> {
    let close_match = DETAILS_CLOSE_RE.find_iter(text).last()?;
    if !text[close_match.end()..].trim().is_empty() {
        return None;
    }

    let open_match = DETAILS_OPEN_RE
        .find_iter(&text[..close_match.start()])
        .last()?;
    let block_text = &text[open_match.start()..close_match.end()];
    let sources = extract_sources_from_text(block_text);
    if sources.len() < 2 {
        return None;
    }

    let answer = text[..open_match.start()].trim_end().to_string();
    Some((answer, sources))
}

fn split_tail_link_block(text: &str) -> Option<(String, Vec<Source>)> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let mut idx = lines.len() as isize - 1;
    while idx >= 0 && lines[idx as usize].trim().is_empty() {
        idx -= 1;
    }
    if idx < 0 {
        return None;
    }

    let tail_end = idx as usize;
    let mut link_like_count = 0usize;

    while idx >= 0 {
        let line = lines[idx as usize].trim();
        if line.is_empty() {
            idx -= 1;
            continue;
        }

        if !is_link_only_line(line) {
            break;
        }

        link_like_count += 1;
        idx -= 1;
    }

    let tail_start = (idx + 1) as usize;
    if link_like_count < 2 {
        return None;
    }

    let block_text = lines[tail_start..=tail_end].join("\n");
    let sources = extract_sources_from_text(&block_text);
    if sources.is_empty() {
        return None;
    }

    let answer = lines[..tail_start].join("\n").trim_end().to_string();
    Some((answer, sources))
}

fn is_link_only_line(line: &str) -> bool {
    let stripped = LIST_PREFIX_RE.replace(line, "");
    let stripped = stripped.trim();
    if stripped.is_empty() {
        return false;
    }

    stripped.starts_with("http://")
        || stripped.starts_with("https://")
        || MD_LINK_RE.is_match(stripped)
}

fn parse_sources_payload(payload: &str) -> Vec<Source> {
    let payload = payload.trim().trim_end_matches(';').trim();
    if payload.is_empty() {
        return Vec::new();
    }

    let data = parse_json_like_payload(payload);
    if let Some(value) = data {
        if let Value::Object(map) = &value {
            for key in ["sources", "citations", "references", "urls"] {
                if let Some(inner) = map.get(key) {
                    return normalize_sources_value(inner);
                }
            }
        }
        return normalize_sources_value(&value);
    }

    extract_sources_from_text(payload)
}

fn parse_json_like_payload(payload: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(payload) {
        return Some(value);
    }

    let normalized_literals = payload
        .replace("None", "null")
        .replace("True", "true")
        .replace("False", "false");

    let converted =
        SINGLE_QUOTED_STRING_RE.replace_all(&normalized_literals, |caps: &regex::Captures<'_>| {
            let inner = caps.get(1).map_or("", |m| m.as_str());
            let escaped = inner.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        });

    serde_json::from_str::<Value>(converted.as_ref()).ok()
}

fn normalize_sources_value(value: &Value) -> Vec<Source> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();

    match value {
        Value::Array(items) => {
            for item in items {
                normalize_source_item(item, &mut normalized, &mut seen);
            }
        }
        _ => normalize_source_item(value, &mut normalized, &mut seen),
    }

    normalized
}

fn normalize_source_item(item: &Value, normalized: &mut Vec<Source>, seen: &mut HashSet<String>) {
    match item {
        Value::String(text) => {
            for url in extract_unique_urls(text) {
                push_unique_source(normalized, seen, String::new(), &url);
            }
        }
        Value::Array(tuple_like) => {
            if tuple_like.len() >= 2 {
                let title = tuple_like
                    .first()
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("")
                    .to_string();
                let url = tuple_like
                    .get(1)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");

                push_unique_source(normalized, seen, title, url);
            }
        }
        Value::Object(map) => {
            let url = map
                .get("url")
                .or_else(|| map.get("href"))
                .or_else(|| map.get("link"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            let title = map
                .get("title")
                .or_else(|| map.get("name"))
                .or_else(|| map.get("label"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
                .to_string();

            push_unique_source(normalized, seen, title, url);
        }
        _ => {}
    }
}

fn extract_sources_from_text(text: &str) -> Vec<Source> {
    let mut sources = Vec::new();
    let mut seen = HashSet::new();

    for caps in MD_LINK_RE.captures_iter(text) {
        let title = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        let url = caps.get(2).map_or("", |m| m.as_str()).trim();
        push_unique_source(&mut sources, &mut seen, title, url);
    }

    for url in extract_unique_urls(text) {
        push_unique_source(&mut sources, &mut seen, String::new(), &url);
    }

    sources
}

fn push_unique_source(
    sources: &mut Vec<Source>,
    seen: &mut HashSet<String>,
    title: String,
    url: &str,
) {
    let clean_url = url.trim();
    if !is_http_url(clean_url) {
        return;
    }

    if seen.insert(clean_url.to_string()) {
        sources.push(Source {
            title: title.trim().to_string(),
            url: clean_url.to_string(),
        });
    }
}

fn dedup_sources(sources: Vec<Source>) -> Vec<Source> {
    let mut deduped = Vec::new();
    let mut seen = HashSet::new();

    for source in sources {
        let url = source.url.trim();
        if !is_http_url(url) {
            continue;
        }

        if seen.insert(url.to_string()) {
            deduped.push(Source {
                title: source.title.trim().to_string(),
                url: url.to_string(),
            });
        }
    }

    deduped
}

fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn trim_url_tail_punctuation(url: &str) -> &str {
    url.trim_end_matches(|c| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sources_heading() {
        let text = "回答正文\n\n## Sources\n- [Rust 官网](https://www.rust-lang.org)\n- https://example.com/docs";
        let sources = parse_sources(text);

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].title, "Rust 官网");
        assert_eq!(sources[0].url, "https://www.rust-lang.org");
        assert_eq!(sources[1].title, "");
        assert_eq!(sources[1].url, "https://example.com/docs");
    }

    #[test]
    fn test_parse_sources_function_call() {
        let text = "这是回答。\n\nsource_cards([{\"title\":\"Rust 官网\",\"url\":\"https://www.rust-lang.org\"},{\"url\":\"https://example.com\"}])";
        let sources = parse_sources(text);

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].title, "Rust 官网");
        assert_eq!(sources[0].url, "https://www.rust-lang.org");
        assert_eq!(sources[1].title, "");
        assert_eq!(sources[1].url, "https://example.com");
    }

    #[test]
    fn test_parse_sources_details_block() {
        let text = "回答正文\n\n<details><summary>参考来源</summary>\n- [Rust](https://www.rust-lang.org)\n- https://example.com\n</details>";
        let sources = parse_sources(text);

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].url, "https://www.rust-lang.org");
        assert_eq!(sources[1].url, "https://example.com");
        assert_eq!(strip_sources(text), "回答正文");
    }

    #[test]
    fn test_parse_sources_tail_link_block() {
        let text = "回答正文\n\n- [Rust](https://www.rust-lang.org)\n- https://example.com";
        let sources = parse_sources(text);

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].url, "https://www.rust-lang.org");
        assert_eq!(sources[1].url, "https://example.com");
        assert_eq!(strip_sources(text), "回答正文");
    }

    #[test]
    fn test_strip_sources() {
        let text = "正文第一段。\n\n## 参考资料\n1. [Rust](https://www.rust-lang.org)\n2. https://example.com";
        let answer = strip_sources(text);

        assert_eq!(answer, "正文第一段。");
    }

    #[test]
    fn test_extract_unique_urls() {
        let text = "A: https://a.com, B: https://b.com。 C: https://a.com!";
        let urls = extract_unique_urls(text);

        assert_eq!(urls, vec!["https://a.com", "https://b.com"]);
    }

    #[test]
    fn test_parse_sources_empty() {
        assert!(parse_sources("").is_empty());
        assert!(parse_sources("   \n \t").is_empty());
        assert_eq!(strip_sources("   \n \t"), "");
    }

    #[test]
    fn test_no_println() {
        let source = include_str!("parser.rs");
        for name in ["println", "print", "dbg"] {
            let pattern = format!("{name}!(");
            assert!(!source.contains(&pattern), "发现禁用宏: {name}!");
        }
    }
}
