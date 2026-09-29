//! Web 記事を取得して本文を取り出す（Mozilla Readability 相当の dom_smoothie を使用）

use std::time::Duration;

use anyhow::{bail, Context, Result};
use dom_smoothie::Readability;

use crate::models::{NewItem, KIND_ARTICLE};

const MAX_BYTES: usize = 10 * 1024 * 1024;
const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

/// 追跡用パラメータとフラグメントを除いた URL（重複判定に使う）
pub fn normalize_url(raw: &str) -> Result<String> {
    let mut u = url::Url::parse(raw.trim()).context("URL の形式が正しくありません")?;
    if !matches!(u.scheme(), "http" | "https") {
        bail!("http / https の URL を指定してください");
    }
    u.set_fragment(None);
    let kept: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| {
            let k = k.to_ascii_lowercase();
            !(k.starts_with("utm_")
                || matches!(
                    k.as_str(),
                    "fbclid" | "gclid" | "igshid" | "mc_cid" | "mc_eid" | "ref_src"
                ))
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut().clear().extend_pairs(kept);
    }
    Ok(u.to_string())
}

pub fn article_key(normalized_url: &str) -> String {
    format!("url:{normalized_url}")
}

/// Content-Type か <meta charset> から文字コードを判定して文字列にする
fn decode_html(bytes: &[u8], content_type: Option<&str>) -> String {
    let from_header = content_type.and_then(|ct| {
        ct.to_ascii_lowercase()
            .split("charset=")
            .nth(1)
            .map(|s| s.trim_matches(['"', ' ', ';']).to_string())
    });
    let from_meta = || {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
        let idx = head.find("charset=")?;
        let rest = &head[idx + 8..];
        let rest = rest.trim_start_matches(['"', '\'']);
        let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))?;
        Some(rest[..end].to_string())
    };
    let label = from_header.or_else(from_meta).unwrap_or_else(|| "utf-8".into());
    let enc = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = enc.decode(bytes);
    text.into_owned()
}

fn clean_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_run = 0;
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank_run += 1;
            if blank_run == 1 && !out.is_empty() {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

fn sanitize(html: &str, base: &url::Url) -> String {
    ammonia::Builder::default()
        .url_relative(ammonia::UrlRelative::RewriteWithBase(base.clone()))
        .link_rel(Some("noopener noreferrer nofollow"))
        .clean(html)
        .to_string()
}

fn absolutize(base: &url::Url, maybe_relative: Option<String>) -> Option<String> {
    let s = maybe_relative?;
    base.join(&s)
        .ok()
        .map(|u| u.to_string())
        .filter(|u| u.starts_with("http"))
}

fn excerpt_from(text: &str) -> Option<String> {
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        return None;
    }
    let mut e: String = t.chars().take(160).collect();
    if t.chars().count() > 160 {
        e.push('…');
    }
    Some(e)
}

/// HTML から記事を組み立てる（ネットワークなしでテストできるよう分けてある）
pub fn extract(html: &str, page_url: &str, normalized_url: &str) -> NewItem {
    let base = url::Url::parse(page_url).unwrap_or_else(|_| url::Url::parse(normalized_url).unwrap());
    let host = base.host_str().unwrap_or("").trim_start_matches("www.").to_string();

    let parsed = Readability::new(html, Some(page_url), None).and_then(|mut r| r.parse());
    match parsed {
        Ok(a) => {
            let text = clean_text(&a.text_content);
            let content_html = sanitize(&a.content, &base);
            let title = Some(a.title.trim().to_string()).filter(|t| !t.is_empty());
            NewItem {
                key: article_key(normalized_url),
                kind: KIND_ARTICLE.into(),
                url: normalized_url.to_string(),
                title,
                author_name: a.byline.map(|b| b.trim().to_string()).filter(|b| !b.is_empty()),
                site_name: a.site_name.filter(|s| !s.trim().is_empty()).or(Some(host)),
                excerpt: a
                    .excerpt
                    .filter(|e| !e.trim().is_empty())
                    .or_else(|| excerpt_from(&text)),
                image_url: absolutize(&base, a.image),
                published_at: a.published_time.filter(|p| !p.trim().is_empty()),
                lang: a.lang,
                content_html: Some(content_html).filter(|h| !h.trim().is_empty()),
                text,
                ..Default::default()
            }
        }
        Err(_) => {
            // 本文を抽出できないページは、タイトルだけでも保存する
            let title = html
                .find("<title")
                .and_then(|i| html[i..].find('>').map(|j| i + j + 1))
                .and_then(|s| html[s..].find("</title>").map(|e| html[s..s + e].trim().to_string()))
                .filter(|t| !t.is_empty());
            NewItem {
                key: article_key(normalized_url),
                kind: KIND_ARTICLE.into(),
                url: normalized_url.to_string(),
                title: title.or_else(|| Some(normalized_url.to_string())),
                site_name: Some(host),
                ..Default::default()
            }
        }
    }
}

pub async fn fetch(http: &reqwest::Client, raw_url: &str) -> Result<NewItem> {
    let normalized = normalize_url(raw_url)?;
    let resp = http
        .get(&normalized)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(
            reqwest::header::ACCEPT,
            "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8",
        )
        .header(reqwest::header::ACCEPT_LANGUAGE, "ja,en;q=0.8")
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .with_context(|| format!("ページを取得できません: {normalized}"))?;
    let status = resp.status();
    if !status.is_success() {
        bail!("ページを取得できません ({status}): {normalized}");
    }
    let final_url = resp.url().to_string();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    if let Some(ct) = &content_type {
        let ct = ct.to_ascii_lowercase();
        if !(ct.contains("html") || ct.contains("xml") || ct.starts_with("text/")) {
            bail!("HTML ページではありません（{ct}）");
        }
    }
    let bytes = resp.bytes().await?;
    if bytes.len() > MAX_BYTES {
        bail!("ページが大きすぎます（{} MB）", bytes.len() / 1024 / 1024);
    }
    let html = decode_html(&bytes, content_type.as_deref());
    Ok(extract(&html, &final_url, &normalized))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<!doctype html>
<html lang="ja"><head>
<meta charset="utf-8">
<title>Rust で作るデスクトップアプリ | テックブログ</title>
<meta property="og:title" content="Rust で作るデスクトップアプリ">
<meta property="og:site_name" content="テックブログ">
<meta property="og:image" content="/images/cover.png">
<meta property="article:published_time" content="2026-09-01T09:00:00+09:00">
<meta name="author" content="山田花子">
</head><body>
<nav><a href="/">ホーム</a></nav>
<article>
<h1>Rust で作るデスクトップアプリ</h1>
<p>Tauri を使うと、Rust と Web 技術でデスクトップアプリを作れます。このページでは基本的な構成を紹介します。軽量で安全なアプリが作れるのが特徴です。</p>
<p>フロントエンドには React を使い、バックエンドの処理は Rust で書きます。<a href="/next">次の記事</a>も参考にしてください。</p>
<script>alert('x')</script>
<p onclick="evil()">データの保存には SQLite を使います。全文検索もできます。ここまで読んでいただきありがとうございました。</p>
<img src="/images/diagram.png" onerror="evil()" alt="構成図">
</article>
<footer>© 2026</footer>
</body></html>"#;

    #[test]
    fn extracts_article_and_sanitizes() {
        let item = extract(
            PAGE,
            "https://blog.example.com/posts/tauri",
            "https://blog.example.com/posts/tauri",
        );
        assert_eq!(item.kind, "article");
        assert_eq!(item.key, "url:https://blog.example.com/posts/tauri");
        assert!(item.title.as_deref().unwrap().contains("Rust で作るデスクトップアプリ"));
        assert_eq!(item.site_name.as_deref(), Some("テックブログ"));
        assert_eq!(
            item.image_url.as_deref(),
            Some("https://blog.example.com/images/cover.png")
        );
        assert!(item.text.contains("Tauri を使うと"));
        assert!(item.text.contains("SQLite"));
        let html = item.content_html.unwrap();
        assert!(!html.contains("<script"), "script は除去");
        assert!(
            !html.contains("onclick") && !html.contains("onerror"),
            "イベント属性は除去"
        );
        assert!(
            html.contains("https://blog.example.com/next"),
            "相対リンクは絶対 URL に"
        );
        assert!(html.contains("https://blog.example.com/images/diagram.png"));
        assert!(item.published_at.is_some());
    }

    #[test]
    fn normalizes_tracking_params() {
        assert_eq!(
            normalize_url("https://example.com/a?utm_source=x&id=3&fbclid=abc#top").unwrap(),
            "https://example.com/a?id=3"
        );
        assert_eq!(
            normalize_url("https://example.com/a?utm_medium=y").unwrap(),
            "https://example.com/a"
        );
        assert!(normalize_url("ftp://example.com").is_err());
        assert!(normalize_url("not a url").is_err());
    }

    #[test]
    fn decodes_shift_jis_from_meta() {
        let (bytes, _, _) =
            encoding_rs::SHIFT_JIS.encode("<html><head><meta charset=\"Shift_JIS\"></head><body>日本語</body></html>");
        let s = decode_html(&bytes, Some("text/html"));
        assert!(s.contains("日本語"));
        let s2 = decode_html(&bytes, Some("text/html; charset=Shift_JIS"));
        assert!(s2.contains("日本語"));
    }

    #[test]
    fn falls_back_to_title_when_unreadable() {
        let item = extract(
            "<html><head><title>短いページ</title></head><body></body></html>",
            "https://e.com/x",
            "https://e.com/x",
        );
        assert_eq!(item.title.as_deref(), Some("短いページ"));
    }
}
