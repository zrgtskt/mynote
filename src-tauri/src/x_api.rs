//! X (旧 Twitter) API v2 クライアント
//!
//! - 認可: OAuth 2.0 Authorization Code + PKCE。ブラウザで許可し、
//!   http://127.0.0.1:8723/callback でコードを受け取る。
//! - 取得: 自分のいいね (`/2/users/:id/liked_tweets`) とブックマーク (`/2/users/:id/bookmarks`)。
//!   従量課金では自分のデータの取得は「Owned Reads」として 1 件ごとに課金される。

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::models::{LinkCard, Media, NewItem, KIND_TWEET};

pub const REDIRECT_PORT: u16 = 8723;
pub const REDIRECT_URI: &str = "http://127.0.0.1:8723/callback";
const AUTHORIZE_URL: &str = "https://x.com/i/oauth2/authorize";
const TOKEN_URL: &str = "https://api.x.com/2/oauth2/token";
const API_BASE: &str = "https://api.x.com/2";
const SCOPES: &str = "tweet.read users.read like.read bookmark.read offline.access";

const TWEET_FIELDS: &str = "created_at,author_id,entities,attachments,note_tweet,public_metrics,lang";
const EXPANSIONS: &str = "author_id,attachments.media_keys";
const USER_FIELDS: &str = "name,username,profile_image_url";
const MEDIA_FIELDS: &str = "type,url,preview_image_url,width,height,alt_text,variants";

// ------------------------------------------------------------------ OAuth

#[derive(Clone, Debug)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// UNIX 秒
    pub expires_at: i64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn new_pkce() -> Pkce {
    let verifier = random_token(48);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Pkce {
        verifier,
        challenge,
        state: random_token(16),
    }
}

pub fn authorize_url(client_id: &str, pkce: &Pkce) -> String {
    let mut url = url::Url::parse(AUTHORIZE_URL).unwrap();
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPES)
        .append_pair("state", &pkce.state)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256");
    url.to_string()
}

/// コールバック用のローカルサーバーを先に立てておく（ポート使用中ならここで失敗させる）
pub async fn bind_callback() -> Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", REDIRECT_PORT))
        .await
        .with_context(|| format!("ポート {REDIRECT_PORT} を使えません。他のアプリが使っていないか確認してください"))
}

/// ブラウザからのリダイレクトを 1 回受け取り、認可コードを返す
pub async fn wait_for_code(listener: TcpListener, expected_state: &str, timeout: Duration) -> Result<String> {
    let fut = async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await?;
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            if !path.starts_with("/callback") {
                let _ = sock
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                continue;
            }
            let url = url::Url::parse(&format!("http://127.0.0.1{path}"))?;
            let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
            let result = if let Some(err) = params.get("error") {
                Err(anyhow!("X での認可が拒否されました: {err}"))
            } else if params.get("state").map(String::as_str) != Some(expected_state) {
                Err(anyhow!("認可の state が一致しません。もう一度やり直してください"))
            } else {
                params
                    .get("code")
                    .cloned()
                    .ok_or_else(|| anyhow!("認可コードがありません"))
            };
            let (title, body) = match &result {
                Ok(_) => (
                    "連携しました",
                    "mynote と X の連携が完了しました。このタブを閉じてアプリに戻ってください。",
                ),
                Err(_) => (
                    "連携できませんでした",
                    "X との連携に失敗しました。アプリに戻ってもう一度お試しください。",
                ),
            };
            let html = format!(
                "<!doctype html><meta charset=utf-8><title>{title}</title>\
                 <body style=\"font-family:system-ui,sans-serif;background:#0b0d17;color:#e8e9f3;display:grid;place-items:center;height:100vh;margin:0\">\
                 <div style=\"text-align:center\"><h1 style=\"font-weight:600\">{title}</h1><p style=\"opacity:.7\">{body}</p></div>"
            );
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                html.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;
            return result;
        }
    };
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| anyhow!("X での認可がタイムアウトしました（5 分）"))?
}

fn token_request(http: &reqwest::Client, client_id: &str, client_secret: Option<&str>) -> reqwest::RequestBuilder {
    let req = http.post(TOKEN_URL);
    match client_secret {
        Some(secret) if !secret.is_empty() => req.basic_auth(client_id, Some(secret)),
        _ => req,
    }
}

async fn parse_token_response(resp: reqwest::Response) -> Result<Tokens> {
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        bail!("トークンの取得に失敗しました ({status}): {}", error_detail(&body));
    }
    let t: TokenResponse = serde_json::from_str(&body).context("トークン応答を解析できません")?;
    Ok(Tokens {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: Utc::now().timestamp() + t.expires_in.unwrap_or(7200),
    })
}

pub async fn exchange_code(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    verifier: &str,
) -> Result<Tokens> {
    let resp = token_request(http, client_id, client_secret)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", verifier),
            ("client_id", client_id),
        ])
        .send()
        .await?;
    parse_token_response(resp).await
}

pub async fn refresh(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<Tokens> {
    let resp = token_request(http, client_id, client_secret)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ])
        .send()
        .await?;
    parse_token_response(resp).await
}

// ------------------------------------------------------------------ API

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    RateLimited { reset_at: Option<i64> },
    PaymentRequired(String),
    Other(anyhow::Error),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Unauthorized => write!(f, "X の認証が切れています。設定から再連携してください"),
            ApiError::RateLimited { reset_at } => match reset_at {
                Some(t) => {
                    let mins = ((t - Utc::now().timestamp()).max(0) + 59) / 60;
                    write!(f, "X API の回数制限に達しました。約 {mins} 分後に再試行してください")
                }
                None => write!(f, "X API の回数制限に達しました。しばらくしてから再試行してください"),
            },
            ApiError::PaymentRequired(d) => write!(
                f,
                "X API のクレジットが不足しています（開発者ポータルでチャージしてください）: {d}"
            ),
            ApiError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::Other(e.into())
    }
}

fn error_detail(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        for key in ["detail", "error_description", "title", "error"] {
            if let Some(s) = v.get(key).and_then(|s| s.as_str()) {
                return s.to_string();
            }
        }
    }
    body.chars().take(300).collect()
}

pub async fn get_json(
    http: &reqwest::Client,
    token: &str,
    path: &str,
    query: &[(&str, String)],
) -> Result<String, ApiError> {
    let resp = http
        .get(format!("{API_BASE}{path}"))
        .bearer_auth(token)
        .query(query)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    let status = resp.status();
    let reset_at = resp
        .headers()
        .get("x-rate-limit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());
    let body = resp.text().await?;
    match status.as_u16() {
        200..=299 => Ok(body),
        401 => Err(ApiError::Unauthorized),
        402 => Err(ApiError::PaymentRequired(error_detail(&body))),
        429 => Err(ApiError::RateLimited { reset_at }),
        _ => Err(ApiError::Other(anyhow!(
            "X API エラー ({status}): {}",
            error_detail(&body)
        ))),
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct XUser {
    pub id: String,
    pub name: String,
    pub username: String,
    pub profile_image_url: Option<String>,
}

#[derive(Deserialize)]
struct MeResponse {
    data: XUser,
}

pub async fn get_me(http: &reqwest::Client, token: &str) -> Result<XUser, ApiError> {
    let body = get_json(http, token, "/users/me", &[("user.fields", "profile_image_url".into())]).await?;
    let me: MeResponse = serde_json::from_str(&body).map_err(|e| ApiError::Other(e.into()))?;
    Ok(me.data)
}

/// いいね / ブックマークの種類
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Collection {
    Likes,
    Bookmarks,
}

impl Collection {
    pub fn path(&self, user_id: &str) -> String {
        match self {
            Collection::Likes => format!("/users/{user_id}/liked_tweets"),
            Collection::Bookmarks => format!("/users/{user_id}/bookmarks"),
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Collection::Likes => "いいね",
            Collection::Bookmarks => "ブックマーク",
        }
    }
}

pub fn page_query(max_results: i64, pagination_token: Option<&str>) -> Vec<(&'static str, String)> {
    let mut q = vec![
        ("max_results", max_results.to_string()),
        ("tweet.fields", TWEET_FIELDS.to_string()),
        ("expansions", EXPANSIONS.to_string()),
        ("user.fields", USER_FIELDS.to_string()),
        ("media.fields", MEDIA_FIELDS.to_string()),
    ];
    if let Some(t) = pagination_token {
        q.push(("pagination_token", t.to_string()));
    }
    q
}

pub fn single_tweet_query() -> Vec<(&'static str, String)> {
    vec![
        ("tweet.fields", TWEET_FIELDS.to_string()),
        ("expansions", EXPANSIONS.to_string()),
        ("user.fields", USER_FIELDS.to_string()),
        ("media.fields", MEDIA_FIELDS.to_string()),
    ]
}

// ------------------------------------------------------------------ parsing

#[derive(Deserialize, Debug, Default)]
pub struct TweetsResponse {
    pub data: Option<Vec<TweetData>>,
    pub includes: Option<Includes>,
    pub meta: Option<Meta>,
}

#[derive(Deserialize, Debug)]
pub struct SingleTweetResponse {
    pub data: Option<TweetData>,
    pub includes: Option<Includes>,
}

#[derive(Deserialize, Debug, Clone, serde::Serialize)]
pub struct TweetData {
    pub id: String,
    pub text: String,
    pub author_id: Option<String>,
    pub created_at: Option<String>,
    pub lang: Option<String>,
    pub entities: Option<Entities>,
    pub attachments: Option<Attachments>,
    pub note_tweet: Option<NoteTweet>,
    pub public_metrics: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug, Clone, serde::Serialize)]
pub struct NoteTweet {
    pub text: String,
    pub entities: Option<Entities>,
}

#[derive(Deserialize, Debug, Clone, Default, serde::Serialize)]
pub struct Entities {
    #[serde(default)]
    pub urls: Vec<UrlEntity>,
}

#[derive(Deserialize, Debug, Clone, serde::Serialize)]
pub struct UrlEntity {
    pub url: String,
    pub expanded_url: Option<String>,
    pub unwound_url: Option<String>,
    pub display_url: Option<String>,
    pub media_key: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub images: Vec<UrlImage>,
}

#[derive(Deserialize, Debug, Clone, serde::Serialize)]
pub struct UrlImage {
    pub url: String,
    pub width: Option<i64>,
}

#[derive(Deserialize, Debug, Clone, serde::Serialize)]
pub struct Attachments {
    #[serde(default)]
    pub media_keys: Vec<String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct Includes {
    #[serde(default)]
    pub users: Vec<XUser>,
    #[serde(default)]
    pub media: Vec<MediaData>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MediaData {
    pub media_key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: Option<String>,
    pub preview_image_url: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub alt_text: Option<String>,
    #[serde(default)]
    pub variants: Vec<Variant>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Variant {
    pub bit_rate: Option<i64>,
    pub content_type: String,
    pub url: String,
}

#[derive(Deserialize, Debug, Default)]
pub struct Meta {
    pub next_token: Option<String>,
}

pub fn tweet_key(id: &str) -> String {
    format!("tweet:{id}")
}

fn bigger_avatar(url: &str) -> String {
    url.replace("_normal.", "_bigger.")
}

/// t.co の短縮 URL を展開し、画像への t.co は本文から取り除く
fn expand_text(text: &str, entities: Option<&Entities>) -> String {
    let mut out = text.to_string();
    if let Some(ent) = entities {
        for u in &ent.urls {
            let is_media = u.media_key.is_some()
                || u.expanded_url
                    .as_deref()
                    .map(|e| e.contains("/photo/") || e.contains("/video/"))
                    .unwrap_or(false);
            let replacement = if is_media {
                String::new()
            } else {
                u.unwound_url
                    .clone()
                    .or_else(|| u.expanded_url.clone())
                    .unwrap_or_else(|| u.url.clone())
            };
            out = out.replace(&u.url, &replacement);
        }
    }
    // HTML エンティティ（&amp; など）を戻す
    let out = out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">");
    out.trim().to_string()
}

/// X 自身（ポストやプロフィール）へのリンクか
pub fn is_x_url(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .map(|h| {
            let h = h.trim_start_matches("www.").trim_start_matches("mobile.");
            matches!(h, "x.com" | "twitter.com" | "t.co" | "pic.twitter.com" | "pic.x.com")
        })
        .unwrap_or(false)
}

/// ポストが紹介している外部リンク。プレビュー情報があるものを優先し、なければ最初の外部リンク。
fn link_card(entities: Option<&Entities>) -> Option<LinkCard> {
    let ent = entities?;
    let external: Vec<&UrlEntity> = ent
        .urls
        .iter()
        .filter(|u| u.media_key.is_none())
        .filter(|u| !is_x_url(u.expanded_url.as_deref().unwrap_or(&u.url)))
        .collect();
    external
        .iter()
        .find(|u| u.title.is_some() || !u.images.is_empty())
        .or(external.first())
        .map(|u| LinkCard {
            url: u
                .unwound_url
                .clone()
                .or_else(|| u.expanded_url.clone())
                .unwrap_or_else(|| u.url.clone()),
            title: u.title.clone(),
            description: u.description.clone(),
            // 一番大きいプレビュー画像
            image: u
                .images
                .iter()
                .max_by_key(|i| i.width.unwrap_or(0))
                .map(|i| i.url.clone()),
            image_local: None,
        })
}

fn convert_media(m: &MediaData) -> Media {
    let video_url = m
        .variants
        .iter()
        .filter(|v| v.content_type == "video/mp4")
        .max_by_key(|v| v.bit_rate.unwrap_or(0))
        .map(|v| v.url.clone());
    Media {
        kind: m.kind.clone(),
        url: m.url.clone(),
        preview_url: m.preview_image_url.clone(),
        video_url,
        width: m.width,
        height: m.height,
        alt: m.alt_text.clone(),
        local_path: None,
    }
}

pub fn tweet_to_item(t: &TweetData, includes: &Includes) -> NewItem {
    let author = t
        .author_id
        .as_ref()
        .and_then(|aid| includes.users.iter().find(|u| &u.id == aid));
    let (text, entities) = match &t.note_tweet {
        // 長文ポストは note_tweet に全文が入る
        Some(n) => (n.text.as_str(), n.entities.as_ref().or(t.entities.as_ref())),
        None => (t.text.as_str(), t.entities.as_ref()),
    };
    let media: Vec<Media> = t
        .attachments
        .as_ref()
        .map(|a| {
            a.media_keys
                .iter()
                .filter_map(|k| includes.media.iter().find(|m| &m.media_key == k))
                .map(convert_media)
                .collect()
        })
        .unwrap_or_default();
    let url = match author {
        Some(a) => format!("https://x.com/{}/status/{}", a.username, t.id),
        None => format!("https://x.com/i/web/status/{}", t.id),
    };
    NewItem {
        key: tweet_key(&t.id),
        kind: KIND_TWEET.into(),
        url,
        external_id: Some(t.id.clone()),
        title: None,
        author_name: author.map(|a| a.name.clone()),
        author_handle: author.map(|a| a.username.clone()),
        author_avatar: author.and_then(|a| a.profile_image_url.as_deref().map(bigger_avatar)),
        site_name: Some("X".into()),
        text: expand_text(text, entities),
        content_html: None,
        excerpt: None,
        image_url: None,
        media,
        link: link_card(entities),
        metrics: t.public_metrics.clone(),
        lang: t.lang.clone(),
        published_at: t.created_at.clone(),
        raw_json: serde_json::to_string(t).ok(),
    }
}

pub fn parse_page(body: &str) -> Result<(Vec<NewItem>, Option<String>)> {
    let resp: TweetsResponse = serde_json::from_str(body).context("X API の応答を解析できません")?;
    let includes = resp.includes.unwrap_or_default();
    let items = resp
        .data
        .unwrap_or_default()
        .iter()
        .map(|t| tweet_to_item(t, &includes))
        .collect();
    Ok((items, resp.meta.and_then(|m| m.next_token)))
}

pub fn parse_single(body: &str) -> Result<NewItem> {
    let resp: SingleTweetResponse = serde_json::from_str(body).context("X API の応答を解析できません")?;
    let includes = resp.includes.unwrap_or_default();
    let t = resp
        .data
        .ok_or_else(|| anyhow!("ポストが見つかりません（削除済みか非公開の可能性があります）"))?;
    Ok(tweet_to_item(&t, &includes))
}

/// x.com / twitter.com のポスト URL から ID を取り出す
pub fn tweet_id_from_url(url: &str) -> Option<String> {
    let u = url::Url::parse(url.trim()).ok()?;
    let host = u.host_str()?.trim_start_matches("www.").trim_start_matches("mobile.");
    if !matches!(host, "x.com" | "twitter.com") {
        return None;
    }
    let segs: Vec<&str> = u.path_segments()?.collect();
    let pos = segs.iter().position(|s| *s == "status" || *s == "statuses")?;
    let id = segs.get(pos + 1)?;
    if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
        Some(id.to_string())
    } else {
        None
    }
}

// ------------------------------------------------------------------ oEmbed（未連携時の手動追加用）

#[derive(Deserialize)]
struct OEmbed {
    html: String,
    author_name: Option<String>,
    author_url: Option<String>,
}

/// X と連携していなくても、公開ポストなら oEmbed で本文と投稿者を取れる（無料）
pub async fn fetch_oembed(http: &reqwest::Client, tweet_url: &str, id: &str) -> Result<NewItem> {
    let resp = http
        .get("https://publish.twitter.com/oembed")
        .query(&[("url", tweet_url), ("omit_script", "true"), ("dnt", "true")])
        .timeout(Duration::from_secs(20))
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!(
            "ポストを取得できませんでした ({})。非公開か削除済みの可能性があります",
            resp.status()
        );
    }
    let o: OEmbed = resp.json().await?;
    Ok(oembed_to_item(&o.html, o.author_name, o.author_url, id))
}

fn oembed_to_item(html: &str, author_name: Option<String>, author_url: Option<String>, id: &str) -> NewItem {
    let frag = scraper_free_text(html);
    let handle = author_url
        .as_deref()
        .and_then(|u| u.rsplit('/').next())
        .map(str::to_string);
    let url = match &handle {
        Some(h) => format!("https://x.com/{h}/status/{id}"),
        None => format!("https://x.com/i/web/status/{id}"),
    };
    NewItem {
        key: tweet_key(id),
        kind: KIND_TWEET.into(),
        url,
        external_id: Some(id.to_string()),
        author_name,
        author_handle: handle,
        site_name: Some("X".into()),
        text: frag,
        link: oembed_link(html),
        ..Default::default()
    }
}

/// oEmbed の本文にある外部リンク（t.co）を紹介先として取り出す。画像・ハッシュタグ・メンションは除く。
fn oembed_link(html: &str) -> Option<LinkCard> {
    let mut rest = html;
    while let Some(i) = rest.find("<a href=\"") {
        rest = &rest[i + 9..];
        let href_end = rest.find('"')?;
        let href = &rest[..href_end];
        let text_start = rest.find('>')? + 1;
        let text_end = rest[text_start..].find("</a>")? + text_start;
        let text = &rest[text_start..text_end];
        rest = &rest[text_end..];
        let is_pic = text.starts_with("pic.twitter.com") || text.starts_with("pic.x.com");
        let is_x_page = href.contains("://twitter.com/") || href.contains("://x.com/");
        if !is_pic && !is_x_page && href.starts_with("http") {
            return Some(LinkCard {
                url: href.replace("&amp;", "&"),
                ..Default::default()
            });
        }
    }
    None
}

/// oEmbed の blockquote から本文（最初の <p>）をテキストとして取り出す
fn scraper_free_text(html: &str) -> String {
    let p_start = html.find("<p").and_then(|i| html[i..].find('>').map(|j| i + j + 1));
    let p_end = html.find("</p>");
    let inner = match (p_start, p_end) {
        (Some(s), Some(e)) if e > s => &html[s..e],
        _ => html,
    };
    let with_breaks = inner
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    let mut out = String::new();
    let mut in_tag = false;
    for c in with_breaks.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "data": [
        {
          "id": "1800000000000000001",
          "text": "Rust の新しい記事 https://t.co/abc123 すごい &amp; 面白い https://t.co/pic999",
          "author_id": "42",
          "created_at": "2026-09-20T10:00:00.000Z",
          "lang": "ja",
          "attachments": {"media_keys": ["3_1", "7_2"]},
          "entities": {"urls": [
            {"start": 12, "end": 35, "url": "https://t.co/abc123", "expanded_url": "https://blog.example.com/rust", "display_url": "blog.example.com/rust",
             "title": "Rust 入門", "description": "Rust を学ぶ", "images": [{"url": "https://pbs.twimg.com/news_img/s.jpg", "width": 150}, {"url": "https://pbs.twimg.com/news_img/l.jpg", "width": 800}]},
            {"start": 50, "end": 73, "url": "https://t.co/pic999", "expanded_url": "https://x.com/rustacean/status/1800000000000000001/photo/1", "media_key": "3_1"}
          ]},
          "public_metrics": {"like_count": 10, "retweet_count": 2}
        },
        {
          "id": "1800000000000000002",
          "text": "短縮版…",
          "author_id": "43",
          "note_tweet": {"text": "これは長文ポストの全文です。"}
        }
      ],
      "includes": {
        "users": [
          {"id": "42", "name": "Rustacean", "username": "rustacean", "profile_image_url": "https://pbs.twimg.com/profile_images/1/a_normal.jpg"},
          {"id": "43", "name": "長文さん", "username": "longwriter"}
        ],
        "media": [
          {"media_key": "3_1", "type": "photo", "url": "https://pbs.twimg.com/media/p.jpg", "width": 1200, "height": 800},
          {"media_key": "7_2", "type": "video", "preview_image_url": "https://pbs.twimg.com/ext_tw_video_thumb/v.jpg",
           "variants": [
             {"content_type": "application/x-mpegURL", "url": "https://video.twimg.com/v.m3u8"},
             {"bit_rate": 256000, "content_type": "video/mp4", "url": "https://video.twimg.com/low.mp4"},
             {"bit_rate": 2176000, "content_type": "video/mp4", "url": "https://video.twimg.com/high.mp4"}
           ]}
        ]
      },
      "meta": {"result_count": 2, "next_token": "NEXT"}
    }"#;

    #[test]
    fn parses_liked_tweets_page() {
        let (items, next) = parse_page(SAMPLE).unwrap();
        assert_eq!(next.as_deref(), Some("NEXT"));
        assert_eq!(items.len(), 2);

        let a = &items[0];
        assert_eq!(a.key, "tweet:1800000000000000001");
        assert_eq!(a.url, "https://x.com/rustacean/status/1800000000000000001");
        assert_eq!(
            a.text,
            "Rust の新しい記事 https://blog.example.com/rust すごい & 面白い"
        );
        assert_eq!(
            a.author_avatar.as_deref(),
            Some("https://pbs.twimg.com/profile_images/1/a_bigger.jpg")
        );
        assert_eq!(a.media.len(), 2);
        assert_eq!(a.media[0].url.as_deref(), Some("https://pbs.twimg.com/media/p.jpg"));
        assert_eq!(
            a.media[1].video_url.as_deref(),
            Some("https://video.twimg.com/high.mp4")
        );
        assert_eq!(
            a.media[1].image_url(),
            Some("https://pbs.twimg.com/ext_tw_video_thumb/v.jpg")
        );
        let card = a.link.as_ref().unwrap();
        assert_eq!(card.title.as_deref(), Some("Rust 入門"));
        assert_eq!(card.image.as_deref(), Some("https://pbs.twimg.com/news_img/l.jpg"));
        assert_eq!(a.published_at.as_deref(), Some("2026-09-20T10:00:00.000Z"));

        let b = &items[1];
        assert_eq!(b.text, "これは長文ポストの全文です。");
        assert_eq!(b.author_handle.as_deref(), Some("longwriter"));
        assert!(b.author_avatar.is_none());
    }

    #[test]
    fn link_without_preview_is_still_kept() {
        let json = r#"{
          "data": [{"id": "9", "text": "読んだ https://t.co/a と https://t.co/b", "author_id": "1",
            "entities": {"urls": [
              {"url": "https://t.co/a", "expanded_url": "https://x.com/someone/status/1"},
              {"url": "https://t.co/b", "expanded_url": "https://www.box.com/blog/post"}
            ]}}],
          "includes": {"users": [{"id": "1", "name": "n", "username": "u"}]}
        }"#;
        let (items, _) = parse_page(json).unwrap();
        let card = items[0].link.as_ref().unwrap();
        assert_eq!(
            card.url, "https://www.box.com/blog/post",
            "x.com へのリンクは除き、box.com は除かない"
        );
        assert!(card.title.is_none());
    }

    #[test]
    fn detects_x_urls_by_host() {
        assert!(is_x_url("https://x.com/a/status/1"));
        assert!(is_x_url("https://mobile.twitter.com/a"));
        assert!(!is_x_url("https://www.box.com/x.com/"));
        assert!(!is_x_url("https://netflix.com/"));
    }

    #[test]
    fn parses_empty_page() {
        let (items, next) = parse_page(r#"{"meta": {"result_count": 0}}"#).unwrap();
        assert!(items.is_empty() && next.is_none());
    }

    #[test]
    fn extracts_tweet_id() {
        assert_eq!(
            tweet_id_from_url("https://x.com/user/status/123?s=20").as_deref(),
            Some("123")
        );
        assert_eq!(
            tweet_id_from_url("https://twitter.com/user/status/456/photo/1").as_deref(),
            Some("456")
        );
        assert_eq!(
            tweet_id_from_url("https://mobile.twitter.com/u/status/789").as_deref(),
            Some("789")
        );
        assert_eq!(tweet_id_from_url("https://x.com/user"), None);
        assert_eq!(tweet_id_from_url("https://example.com/status/1"), None);
    }

    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        let p = new_pkce();
        assert!(p.verifier.len() >= 43 && p.verifier.len() <= 128);
        assert_eq!(
            p.challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(p.verifier.as_bytes()))
        );
        let url = authorize_url("CID", &p);
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8723%2Fcallback"));
        assert!(url.contains("bookmark.read"));
    }

    #[test]
    fn oembed_text_extraction() {
        let html = r#"<blockquote class="twitter-tweet"><p lang="ja" dir="ltr">こんにちは<br>世界 &amp; <a href="https://t.co/x">https://t.co/x</a></p>&mdash; 名前 (@name) <a href="https://twitter.com/name/status/1">Sep 1, 2026</a></blockquote>"#;
        let item = oembed_to_item(html, Some("名前".into()), Some("https://twitter.com/name".into()), "1");
        assert_eq!(item.text, "こんにちは\n世界 & https://t.co/x");
        assert_eq!(
            item.link.as_ref().map(|l| l.url.as_str()),
            Some("https://t.co/x"),
            "本文のリンクを紹介先にする"
        );
        assert_eq!(item.author_handle.as_deref(), Some("name"));
        assert_eq!(item.url, "https://x.com/name/status/1");
    }

    #[tokio::test]
    async fn callback_server_returns_code() {
        // 実ポートと衝突しないよう、別ポートで同じ処理を確認する
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(wait_for_code(listener, "STATE", Duration::from_secs(5)));
        let mut sock = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        sock.write_all(b"GET /callback?state=STATE&code=THE_CODE HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut resp = String::new();
        sock.read_to_string(&mut resp).await.unwrap();
        assert!(resp.starts_with("HTTP/1.1 200"));
        assert_eq!(server.await.unwrap().unwrap(), "THE_CODE");
    }
}
