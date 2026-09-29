//! Claude API で保存したアイテムにタグと一行要約を付ける
//!
//! Rust には公式 SDK がないので Messages API を HTTP で直接呼ぶ。
//! 出力は structured outputs（`output_config.format`）で JSON に固定する。

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{Item, TagCount};

pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// 記事本文はこの文字数までを分類に使う（全文は送らない）
pub const MAX_BODY_CHARS: usize = 6000;
/// 紹介先ページの本文はこの文字数まで使う
pub const MAX_LINKED_CHARS: usize = 3000;
/// 既存タグは使用数の多い順にこの数まで伝える
const MAX_VOCAB: usize = 300;

const SYSTEM_PROMPT: &str = "あなたは、ユーザーが X（旧 Twitter）や Web で保存したポスト・記事を整理するアシスタントです。\
与えられた 1 件の内容に、あとで探しやすくなるタグを付け、内容を一文で要約してください。

タグのルール:
- 1〜5 個。内容の主題・分野・固有の技術名や作品名など、あとで絞り込みに使える語を選ぶ
- 既存タグの一覧に意味の合うものがあれば、表記を変えずにそのまま使う（表記ゆれを作らない）
- 新しく作るタグは短い名詞にする。日本語を基本とし、固有名詞や技術名は一般的な表記（例: Rust, React, 機械学習, 料理, 投資）
- 「ツイート」「記事」「面白い」「メモ」のような、分類の役に立たない汎用的な語は使わない
- 先頭に # は付けない

要約のルール:
- 日本語で 1 文、60 文字程度まで
- 何についての内容かが一目で分かるように書く";

/// 安全上の理由で Claude が処理を控えた（再試行しても同じ結果になる）
#[derive(Debug)]
pub struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Claude がこの内容の処理を控えました")
    }
}

impl std::error::Error for Refused {}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TagResult {
    pub tags: Vec<String>,
    pub summary: String,
}

fn output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tags": {
                "type": "array",
                "items": {"type": "string"},
                "description": "1〜5 個のタグ"
            },
            "summary": {
                "type": "string",
                "description": "内容の一文要約（日本語）"
            }
        },
        "required": ["tags", "summary"],
        "additionalProperties": false
    })
}

/// effort パラメーターを受け付けるモデルか
fn supports_effort(model: &str) -> bool {
    !(model.contains("haiku")
        || model.contains("sonnet-4-5")
        || model.starts_with("claude-3")
        || model.contains("opus-4-1"))
}

/// サーバー側フォールバック（`fallbacks: "default"`）に対応するモデルか
fn supports_default_fallback(model: &str) -> bool {
    model.starts_with("claude-opus-5") || model.starts_with("claude-fable-5") || model.starts_with("claude-sonnet-5-5")
}

pub fn build_user_prompt(item: &Item, linked_text: Option<&str>, vocab: &[TagCount]) -> String {
    let mut p = String::new();
    let names: Vec<&str> = vocab.iter().take(MAX_VOCAB).map(|t| t.name.as_str()).collect();
    if names.is_empty() {
        p.push_str("既存タグ: （まだありません）\n\n");
    } else {
        p.push_str(&format!("既存タグ: {}\n\n", names.join(", ")));
    }

    p.push_str("<item>\n");
    let kind = if item.kind == "tweet" {
        "X のポスト"
    } else {
        "Web 記事"
    };
    p.push_str(&format!("種類: {kind}\n"));
    if let Some(site) = item.site_name.as_deref().filter(|_| item.kind != "tweet") {
        p.push_str(&format!("サイト: {site}\n"));
    }
    match (&item.author_name, &item.author_handle) {
        (Some(n), Some(h)) => p.push_str(&format!("投稿者: {n} (@{h})\n")),
        (Some(n), None) => p.push_str(&format!("著者: {n}\n")),
        (None, Some(h)) => p.push_str(&format!("投稿者: @{h}\n")),
        _ => {}
    }
    if let Some(t) = &item.title {
        p.push_str(&format!("タイトル: {t}\n"));
    }
    if let Some(link) = &item.link {
        if let Some(t) = &link.title {
            p.push_str(&format!("リンク先: {t}"));
            if let Some(d) = &link.description {
                p.push_str(&format!(" — {d}"));
            }
            p.push('\n');
        }
    }
    if item.kind != "tweet" {
        if let Some(e) = &item.excerpt {
            p.push_str(&format!("概要: {e}\n"));
        }
    }
    let body: String = item.text.chars().take(MAX_BODY_CHARS).collect();
    let truncated = item.text.chars().count() > MAX_BODY_CHARS;
    p.push_str("本文:\n");
    p.push_str(&body);
    if truncated {
        p.push_str(&format!("\n（本文が長いため、先頭 {MAX_BODY_CHARS} 文字のみ）"));
    }
    if let Some(page) = &item.linked {
        if page.error.is_none() {
            p.push_str("\n\n紹介先のページ:\n");
            if let Some(t) = &page.title {
                p.push_str(&format!("タイトル: {t}\n"));
            }
            if let Some(site) = &page.site_name {
                p.push_str(&format!("サイト: {site}\n"));
            }
            if let Some(text) = linked_text.filter(|t| !t.trim().is_empty()) {
                let body: String = text.chars().take(MAX_LINKED_CHARS).collect();
                p.push_str("本文:\n");
                p.push_str(&body);
                if text.chars().count() > MAX_LINKED_CHARS {
                    p.push_str(&format!("\n（先頭 {MAX_LINKED_CHARS} 文字のみ）"));
                }
            } else if let Some(e) = &page.excerpt {
                p.push_str(&format!("概要: {e}"));
            }
        }
    }
    if let Some(note) = &item.note {
        p.push_str(&format!("\nユーザーのメモ: {note}"));
    }
    p.push_str("\n</item>");
    p
}

pub fn build_request(model: &str, item: &Item, linked_text: Option<&str>, vocab: &[TagCount]) -> Value {
    let mut output_config = json!({
        "format": {"type": "json_schema", "schema": output_schema()}
    });
    if supports_effort(model) {
        // 分類なので深く考える必要はない
        output_config["effort"] = json!("low");
    }
    let mut body = json!({
        "model": model,
        "max_tokens": 4000,
        "system": SYSTEM_PROMPT,
        "output_config": output_config,
        "messages": [{"role": "user", "content": build_user_prompt(item, linked_text, vocab)}]
    });
    if supports_default_fallback(model) {
        // 安全分類器で断られたとき、サーバー側で推奨モデルに切り替えて再実行する
        body["fallbacks"] = json!("default");
    }
    body
}

pub fn parse_response(body: &Value) -> Result<TagResult> {
    match body.get("stop_reason").and_then(Value::as_str) {
        Some("refusal") => return Err(Refused.into()),
        Some("max_tokens") => bail!("Claude の応答が途中で切れました"),
        _ => {}
    }
    let text: String = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Claude の応答に content がありません"))?
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect();
    let mut r: TagResult = serde_json::from_str(text.trim()).context("Claude の応答を JSON として読めません")?;
    r.tags.truncate(5);
    r.summary = r.summary.trim().to_string();
    Ok(r)
}

fn api_error_message(status: reqwest::StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| body.chars().take(300).collect());
    match status.as_u16() {
        401 => format!("Anthropic API キーが無効です: {detail}"),
        403 => format!("この API キーではこの操作が許可されていません: {detail}"),
        404 => format!("モデルが見つかりません（設定のモデル名を確認してください）: {detail}"),
        _ => format!("Claude API エラー ({status}): {detail}"),
    }
}

/// 1 件をタグ付けする。429 / 5xx / 529 は待ってから最大 3 回まで再試行する。
pub async fn tag_item(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    item: &Item,
    linked_text: Option<&str>,
    vocab: &[TagCount],
) -> Result<TagResult> {
    let body = build_request(model, item, linked_text, vocab);
    let mut attempt = 0;
    loop {
        attempt += 1;
        let mut req = http
            .post(API_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .timeout(Duration::from_secs(120))
            .json(&body);
        if supports_default_fallback(model) {
            req = req.header("anthropic-beta", "server-side-fallback-2026-07-01");
        }
        let resp = req.send().await;
        let resp = match resp {
            Ok(r) => r,
            Err(_) if attempt < 4 => {
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                continue;
            }
            Err(e) => return Err(e).context("Claude API に接続できません"),
        };
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let text = resp.text().await?;
        if status.is_success() {
            let v: Value = serde_json::from_str(&text).context("Claude API の応答を解析できません")?;
            return parse_response(&v);
        }
        let retryable = status.as_u16() == 429 || status.as_u16() == 529 || status.is_server_error();
        if retryable && attempt < 4 {
            let wait = retry_after.unwrap_or(2u64.pow(attempt + 1)).min(60);
            tokio::time::sleep(Duration::from_secs(wait)).await;
            continue;
        }
        bail!(api_error_message(status, &text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, text: &str) -> Item {
        Item {
            id: 1,
            kind: kind.into(),
            url: "https://x.com/a/status/1".into(),
            external_id: None,
            title: None,
            author_name: Some("Alice".into()),
            author_handle: Some("alice".into()),
            author_avatar: None,
            author_avatar_local: None,
            site_name: Some("X".into()),
            text: text.into(),
            excerpt: None,
            summary: None,
            image_url: None,
            image_local: None,
            media: vec![],
            link: None,
            metrics: None,
            published_at: None,
            saved_at: "2026-09-29T00:00:00.000Z".into(),
            is_liked: true,
            is_bookmarked: false,
            is_manual: false,
            ai_tagged_at: None,
            note: None,
            has_content: false,
            tags: vec![],
            linked: None,
        }
    }

    #[test]
    fn prompt_includes_linked_page() {
        let mut it = item("tweet", "これ良かった https://blog.example.com/a");
        it.linked = Some(crate::models::LinkedPage {
            url: "https://blog.example.com/a".into(),
            title: Some("SQLite 全文検索入門".into()),
            site_name: Some("Blog".into()),
            fetched_at: "2026-09-29T00:00:00Z".into(),
            has_content: true,
            ..Default::default()
        });
        let long = "本".repeat(MAX_LINKED_CHARS + 10);
        let prompt = build_user_prompt(&it, Some(&long), &[]);
        assert!(prompt.contains("紹介先のページ"));
        assert!(prompt.contains("SQLite 全文検索入門"));
        assert!(prompt.contains("先頭 3000 文字のみ"));

        // 取得に失敗した紹介先は使わない
        it.linked.as_mut().unwrap().error = Some("404".into());
        assert!(!build_user_prompt(&it, None, &[]).contains("紹介先のページ"));
    }

    #[test]
    fn request_uses_structured_output_and_fallback_for_default_model() {
        let vocab = vec![TagCount {
            name: "Rust".into(),
            count: 3,
        }];
        let body = build_request(DEFAULT_MODEL, &item("tweet", "Tauri 2 がリリース"), None, &vocab);
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(body["output_config"]["effort"], "low");
        assert_eq!(body["fallbacks"], "default");
        assert!(
            body.get("thinking").is_none(),
            "Opus 5.5 では thinking を無効化できないので送らない"
        );
        let prompt = body["messages"][0]["content"].as_str().unwrap();
        assert!(prompt.contains("既存タグ: Rust"));
        assert!(prompt.contains("Tauri 2 がリリース"));
        assert!(prompt.contains("@alice"));
    }

    #[test]
    fn request_for_haiku_omits_unsupported_params() {
        let body = build_request("claude-haiku-4-5", &item("tweet", "x"), None, &[]);
        assert!(body["output_config"].get("effort").is_none());
        assert!(body.get("fallbacks").is_none());
    }

    #[test]
    fn long_article_body_is_truncated_with_note() {
        let long = "字".repeat(MAX_BODY_CHARS + 100);
        let prompt = build_user_prompt(&item("article", &long), None, &[]);
        assert!(prompt.contains("先頭 6000 文字のみ"));
        assert!(prompt.contains("既存タグ: （まだありません）"));
    }

    #[test]
    fn parses_text_block_json() {
        let resp = json!({
            "stop_reason": "end_turn",
            "content": [
                {"type": "thinking", "thinking": ""},
                {"type": "text", "text": "{\"tags\": [\"Rust\", \"Tauri\", \"a\", \"b\", \"c\", \"d\"], \"summary\": \" Tauri 2 の紹介 \"}"}
            ]
        });
        let r = parse_response(&resp).unwrap();
        assert_eq!(r.tags.len(), 5);
        assert_eq!(r.summary, "Tauri 2 の紹介");
    }

    #[test]
    fn refusal_is_an_error() {
        let resp = json!({"stop_reason": "refusal", "content": []});
        assert!(parse_response(&resp).unwrap_err().downcast_ref::<Refused>().is_some());
    }
}
