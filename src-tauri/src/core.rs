//! アプリの中核処理。GUI（Tauri コマンド）と CLI（`mynote sync` など）の両方から使う。

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::db::{now_iso, Db};
use crate::models::*;
use crate::x_api::{self, ApiError, Collection};
use crate::{article, images, tagger};

pub const IDENTIFIER: &str = "dev.mynote.desktop";

mod keys {
    pub const X_CLIENT_ID: &str = "x.client_id";
    pub const X_CLIENT_SECRET: &str = "x.client_secret";
    pub const X_ACCESS: &str = "x.access_token";
    pub const X_REFRESH: &str = "x.refresh_token";
    pub const X_EXPIRES: &str = "x.expires_at";
    pub const X_USER_ID: &str = "x.user_id";
    pub const X_USERNAME: &str = "x.username";
    pub const X_NAME: &str = "x.name";
    pub const X_AVATAR: &str = "x.avatar";
    pub const X_SYNCED_LIKES: &str = "x.synced.likes";
    pub const X_SYNCED_BOOKMARKS: &str = "x.synced.bookmarks";
    pub const SYNC_LIKES: &str = "sync.likes";
    pub const SYNC_BOOKMARKS: &str = "sync.bookmarks";
    pub const SYNC_MAX: &str = "sync.max_items";
    pub const AUTO_SYNC: &str = "sync.auto";
    pub const LAST_SYNC_AT: &str = "sync.last_at";
    pub const LAST_SYNC_SUMMARY: &str = "sync.last_summary";
    pub const ANTHROPIC_KEY: &str = "claude.api_key";
    pub const CLAUDE_MODEL: &str = "claude.model";
    pub const AUTO_TAG: &str = "claude.auto_tag";
    pub const PICKUP_COUNT: &str = "pickup.count";
    pub const DOWNLOAD_MEDIA: &str = "media.download";
    pub const FETCH_LINKS: &str = "link.fetch";
}

/// 自動同期の間隔（前回から何時間たったら同期するか）
const AUTO_SYNC_HOURS: i64 = 20;
/// 差分同期の最初のページの件数（従量課金なので、新着が少ないときに取りすぎない）
const INCREMENTAL_FIRST_PAGE: i64 = 20;
const MAX_MEDIA_BYTES: usize = 15 * 1024 * 1024;

pub type ProgressFn = Box<dyn Fn(Progress) + Send + Sync>;

/// URL を取得した結果
enum Fetched {
    Item(Box<NewItem>),
    Image { bytes: Vec<u8>, final_url: String },
}

/// 同期・追加のあとの処理の結果
#[derive(Debug, Default)]
pub struct PostReport {
    pub links: i64,
    pub media: i64,
    pub tagged: Option<TagReport>,
}

/// データの保存先。GUI と CLI で同じ場所を使う。
pub fn default_data_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("MYNOTE_DATA_DIR") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let base = dirs::data_dir().ok_or_else(|| anyhow!("データフォルダの場所を決められません"))?;
    Ok(base.join(IDENTIFIER))
}

pub struct Core {
    db: Mutex<Db>,
    pub http: reqwest::Client,
    pub data_dir: PathBuf,
    pub media_dir: PathBuf,
    sync_lock: tokio::sync::Mutex<()>,
    tag_lock: tokio::sync::Mutex<()>,
    link_lock: tokio::sync::Mutex<()>,
    token_lock: tokio::sync::Mutex<()>,
    connect_lock: tokio::sync::Mutex<()>,
    progress: ProgressFn,
}

impl Core {
    pub fn new(data_dir: PathBuf, progress: ProgressFn) -> Result<Self> {
        std::fs::create_dir_all(&data_dir).with_context(|| format!("フォルダを作れません: {}", data_dir.display()))?;
        let media_dir = data_dir.join("media");
        std::fs::create_dir_all(&media_dir)?;
        let db = Db::open(&data_dir.join("mynote.db"))?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("mynote/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .build()?;
        Ok(Self {
            db: Mutex::new(db),
            http,
            data_dir,
            media_dir,
            sync_lock: Default::default(),
            tag_lock: Default::default(),
            link_lock: Default::default(),
            token_lock: Default::default(),
            connect_lock: Default::default(),
            progress,
        })
    }

    pub fn db(&self) -> MutexGuard<'_, Db> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn emit(&self, task: &str, message: impl Into<String>, current: i64, total: i64, done: bool) {
        (self.progress)(Progress {
            task: task.into(),
            message: message.into(),
            current,
            total,
            done,
        });
    }

    // ------------------------------------------------------------ settings

    fn get(&self, key: &str) -> Option<String> {
        self.db().kv_get(key).ok().flatten().filter(|v| !v.is_empty())
    }

    fn get_bool(&self, key: &str, default: bool) -> bool {
        self.get(key).map(|v| v == "1").unwrap_or(default)
    }

    fn get_i64(&self, key: &str, default: i64) -> i64 {
        self.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.db().kv_set(key, value)
    }

    fn set_bool(&self, key: &str, v: bool) -> Result<()> {
        self.set(key, if v { "1" } else { "0" })
    }

    pub fn anthropic_key(&self) -> Option<String> {
        self.get(keys::ANTHROPIC_KEY)
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.trim().is_empty()))
    }

    pub fn claude_model(&self) -> String {
        self.get(keys::CLAUDE_MODEL)
            .unwrap_or_else(|| tagger::DEFAULT_MODEL.to_string())
    }

    pub fn x_connected(&self) -> bool {
        self.get(keys::X_REFRESH).is_some() || self.get(keys::X_ACCESS).is_some()
    }

    pub fn pickup_count(&self) -> i64 {
        self.get_i64(keys::PICKUP_COUNT, 5)
    }

    pub fn settings(&self) -> PublicSettings {
        PublicSettings {
            x_client_id: self.get(keys::X_CLIENT_ID).unwrap_or_default(),
            has_x_client_secret: self.get(keys::X_CLIENT_SECRET).is_some(),
            x_redirect_uri: x_api::REDIRECT_URI.to_string(),
            x_connected: self.x_connected(),
            x_username: self.get(keys::X_USERNAME),
            x_name: self.get(keys::X_NAME),
            x_avatar: self.get(keys::X_AVATAR),
            sync_likes: self.get_bool(keys::SYNC_LIKES, true),
            sync_bookmarks: self.get_bool(keys::SYNC_BOOKMARKS, true),
            sync_max_items: self.get_i64(keys::SYNC_MAX, 1000),
            auto_sync: self.get_bool(keys::AUTO_SYNC, true),
            last_sync_at: self.get(keys::LAST_SYNC_AT),
            last_sync_summary: self.get(keys::LAST_SYNC_SUMMARY),
            has_anthropic_key: self.anthropic_key().is_some(),
            anthropic_key_from_env: self.get(keys::ANTHROPIC_KEY).is_none() && self.anthropic_key().is_some(),
            claude_model: self.claude_model(),
            auto_tag: self.get_bool(keys::AUTO_TAG, true),
            pickup_count: self.pickup_count(),
            download_media: self.get_bool(keys::DOWNLOAD_MEDIA, true),
            fetch_links: self.get_bool(keys::FETCH_LINKS, true),
            data_dir: self.data_dir.display().to_string(),
        }
    }

    pub fn save_settings(&self, p: SettingsPatch) -> Result<PublicSettings> {
        let set_or_delete = |key: &str, v: &str| -> Result<()> {
            let v = v.trim();
            if v.is_empty() {
                self.db().kv_delete(key)
            } else {
                self.set(key, v)
            }
        };
        if let Some(v) = p.x_client_id {
            set_or_delete(keys::X_CLIENT_ID, &v)?;
        }
        if let Some(v) = p.x_client_secret {
            set_or_delete(keys::X_CLIENT_SECRET, &v)?;
        }
        if let Some(v) = p.anthropic_api_key {
            set_or_delete(keys::ANTHROPIC_KEY, &v)?;
        }
        if let Some(v) = p.claude_model {
            set_or_delete(keys::CLAUDE_MODEL, &v)?;
        }
        if let Some(v) = p.sync_likes {
            self.set_bool(keys::SYNC_LIKES, v)?;
        }
        if let Some(v) = p.sync_bookmarks {
            self.set_bool(keys::SYNC_BOOKMARKS, v)?;
        }
        if let Some(v) = p.sync_max_items {
            self.set(keys::SYNC_MAX, &v.clamp(10, 100_000).to_string())?;
        }
        if let Some(v) = p.auto_sync {
            self.set_bool(keys::AUTO_SYNC, v)?;
        }
        if let Some(v) = p.auto_tag {
            self.set_bool(keys::AUTO_TAG, v)?;
        }
        if let Some(v) = p.pickup_count {
            self.set(keys::PICKUP_COUNT, &v.clamp(1, 50).to_string())?;
        }
        if let Some(v) = p.download_media {
            self.set_bool(keys::DOWNLOAD_MEDIA, v)?;
        }
        if let Some(v) = p.fetch_links {
            self.set_bool(keys::FETCH_LINKS, v)?;
        }
        Ok(self.settings())
    }

    // ------------------------------------------------------------ X 連携

    /// ブラウザで X の認可画面を開き、戻ってきたコードでトークンを得る
    pub async fn x_connect<F>(&self, open_browser: F) -> Result<PublicSettings>
    where
        F: FnOnce(&str) -> Result<()> + Send,
    {
        let Ok(_guard) = self.connect_lock.try_lock() else {
            bail!("連携の処理中です。開いたブラウザで X の許可を済ませてください");
        };
        let client_id = self
            .get(keys::X_CLIENT_ID)
            .ok_or_else(|| anyhow!("先に X の Client ID を設定してください"))?;
        let secret = self.get(keys::X_CLIENT_SECRET);
        let pkce = x_api::new_pkce();
        let listener = x_api::bind_callback().await?;
        open_browser(&x_api::authorize_url(&client_id, &pkce))?;
        let code = x_api::wait_for_code(listener, &pkce.state, Duration::from_secs(300)).await?;
        let tokens = x_api::exchange_code(&self.http, &client_id, secret.as_deref(), &code, &pkce.verifier).await?;
        let me = x_api::get_me(&self.http, &tokens.access_token)
            .await
            .map_err(|e| anyhow!("アカウント情報を取得できません: {e}"))?;
        self.store_tokens(&tokens)?;
        self.set(keys::X_USER_ID, &me.id)?;
        self.set(keys::X_USERNAME, &me.username)?;
        self.set(keys::X_NAME, &me.name)?;
        match me.profile_image_url {
            Some(a) => self.set(keys::X_AVATAR, &a.replace("_normal.", "_bigger."))?,
            None => self.db().kv_delete(keys::X_AVATAR)?,
        }
        Ok(self.settings())
    }

    pub fn x_disconnect(&self) -> Result<PublicSettings> {
        for k in [
            keys::X_ACCESS,
            keys::X_REFRESH,
            keys::X_EXPIRES,
            keys::X_USER_ID,
            keys::X_USERNAME,
            keys::X_NAME,
            keys::X_AVATAR,
        ] {
            self.db().kv_delete(k)?;
        }
        Ok(self.settings())
    }

    fn store_tokens(&self, t: &x_api::Tokens) -> Result<()> {
        self.set(keys::X_ACCESS, &t.access_token)?;
        if let Some(r) = &t.refresh_token {
            self.set(keys::X_REFRESH, r)?;
        }
        self.set(keys::X_EXPIRES, &t.expires_at.to_string())
    }

    /// 有効なアクセストークンを返す（期限が近ければ更新する）
    async fn access_token(&self, force_refresh: bool) -> Result<String> {
        let _guard = self.token_lock.lock().await;
        let expires_at = self.get_i64(keys::X_EXPIRES, 0);
        if !force_refresh {
            if let Some(access) = self.get(keys::X_ACCESS) {
                if expires_at - Utc::now().timestamp() > 120 {
                    return Ok(access);
                }
            }
        }
        let refresh = self
            .get(keys::X_REFRESH)
            .ok_or_else(|| anyhow!("X と連携していません。設定から連携してください"))?;
        let client_id = self
            .get(keys::X_CLIENT_ID)
            .ok_or_else(|| anyhow!("X の Client ID が未設定です"))?;
        let secret = self.get(keys::X_CLIENT_SECRET);
        let tokens = x_api::refresh(&self.http, &client_id, secret.as_deref(), &refresh)
            .await
            .context("X のトークンを更新できません。設定から再連携してください")?;
        self.store_tokens(&tokens)?;
        Ok(tokens.access_token)
    }

    /// X API を呼ぶ。401 のときはトークンを更新して 1 回だけ再試行する。
    async fn x_get(&self, path: &str, query: &[(&str, String)]) -> Result<String> {
        let token = self.access_token(false).await?;
        match x_api::get_json(&self.http, &token, path, query).await {
            Ok(body) => Ok(body),
            Err(ApiError::Unauthorized) => {
                let token = self.access_token(true).await?;
                x_api::get_json(&self.http, &token, path, query)
                    .await
                    .map_err(|e| anyhow!(e.to_string()))
            }
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }

    // ------------------------------------------------------------ 同期

    pub fn should_auto_sync(&self) -> bool {
        if !self.get_bool(keys::AUTO_SYNC, true) || !self.x_connected() {
            return false;
        }
        match self
            .get(keys::LAST_SYNC_AT)
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        {
            Some(last) => Utc::now().signed_duration_since(last).num_hours() >= AUTO_SYNC_HOURS,
            None => true,
        }
    }

    /// いいね・ブックマークを取得して保存する。full=false なら前回以降の新着だけ。
    pub async fn sync_x(&self, full: bool) -> Result<(SyncReport, Vec<i64>)> {
        let Ok(_guard) = self.sync_lock.try_lock() else {
            bail!("すでに同期中です");
        };
        if !self.x_connected() {
            bail!("X と連携していません。設定から連携してください");
        }
        let user_id = self
            .get(keys::X_USER_ID)
            .ok_or_else(|| anyhow!("X のユーザー情報がありません。再連携してください"))?;
        let max_items = self.get_i64(keys::SYNC_MAX, 1000);
        let mut report = SyncReport::default();
        let mut new_ids = Vec::new();
        let started = Utc::now();
        let mut order: i64 = 0;

        let mut targets = Vec::new();
        if self.get_bool(keys::SYNC_LIKES, true) {
            targets.push(Collection::Likes);
        }
        if self.get_bool(keys::SYNC_BOOKMARKS, true) {
            targets.push(Collection::Bookmarks);
        }

        for col in targets {
            let synced_key = match col {
                Collection::Likes => keys::X_SYNCED_LIKES,
                Collection::Bookmarks => keys::X_SYNCED_BOOKMARKS,
            };
            let incremental = !full && self.get(synced_key).is_some();
            let flags = match col {
                Collection::Likes => SourceFlags {
                    liked: true,
                    ..Default::default()
                },
                Collection::Bookmarks => SourceFlags {
                    bookmarked: true,
                    ..Default::default()
                },
            };
            let mut token: Option<String> = None;
            let mut fetched: i64 = 0;
            let mut new_count: i64 = 0;
            let mut page_no = 0;
            let result: Result<()> = async {
                loop {
                    page_no += 1;
                    let page_size = if incremental && page_no == 1 {
                        INCREMENTAL_FIRST_PAGE
                    } else {
                        100
                    };
                    let page_size = page_size.min((max_items - fetched).max(10));
                    self.emit(
                        "sync",
                        format!("{}を取得中…（{} 件）", col.label(), fetched),
                        fetched,
                        max_items,
                        false,
                    );
                    let body = self
                        .x_get(&col.path(&user_id), &x_api::page_query(page_size, token.as_deref()))
                        .await?;
                    let (items, next) = x_api::parse_page(&body)?;
                    let rows: Vec<(NewItem, String)> = items
                        .into_iter()
                        .map(|it| {
                            // 取得順（新しい順）を保つため、保存時刻を 1ms ずつずらす
                            let saved = started - chrono::Duration::milliseconds(order);
                            order += 1;
                            (it, saved.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
                        })
                        .collect();
                    let n = rows.len() as i64;
                    let results = self.db().upsert_items(&rows, flags)?;
                    let mut reached_known = false;
                    for r in results {
                        if r.newly_flagged {
                            new_count += 1;
                            new_ids.push(r.id);
                        } else {
                            reached_known = true;
                        }
                    }
                    fetched += n;
                    if incremental && reached_known {
                        break; // 前回の同期分に追いついた
                    }
                    if fetched >= max_items {
                        break;
                    }
                    match next {
                        Some(t) if n > 0 => token = Some(t),
                        _ => break,
                    }
                }
                Ok(())
            }
            .await;
            match col {
                Collection::Likes => {
                    report.liked_fetched = fetched;
                    report.liked_new = new_count;
                }
                Collection::Bookmarks => {
                    report.bookmarks_fetched = fetched;
                    report.bookmarks_new = new_count;
                }
            }
            match result {
                Ok(()) => self.set(synced_key, &now_iso())?,
                Err(e) => report.errors.push(format!("{}: {e}", col.label())),
            }
        }

        self.set(keys::LAST_SYNC_AT, &now_iso())?;
        self.set(keys::LAST_SYNC_SUMMARY, &report.summary())?;
        self.emit("sync", report.summary(), 1, 1, true);
        Ok((report, new_ids))
    }

    /// 同期・追加のあとの処理。紹介先ページの保存 → 画像の保存 → 自動タグ付けの順に行う
    /// （タグ付けに紹介先の内容を使うため）。
    pub async fn postprocess(&self, ids: &[i64]) -> PostReport {
        let mut report = PostReport::default();
        if !ids.is_empty() {
            if self.get_bool(keys::FETCH_LINKS, true) {
                report.links = self.fetch_linked_pages(Some(ids), false).await.unwrap_or(0);
            }
            report.media = self.download_media(Some(ids)).await.unwrap_or(0);
        }
        if self.get_bool(keys::AUTO_TAG, true) && self.anthropic_key().is_some() {
            report.tagged = self.tag_pending(500).await.ok();
        }
        report
    }

    // ------------------------------------------------------------ URL を追加

    pub async fn add_url(&self, raw: &str) -> Result<Item> {
        let raw = raw.trim();
        if raw.is_empty() {
            bail!("URL を入力してください");
        }
        self.emit("add", "取得中…", 0, 1, false);
        let result = self.fetch_url_item(raw).await;
        let item = match result {
            Ok(Fetched::Item(it)) => *it,
            Ok(Fetched::Image { bytes, final_url }) => {
                // URL が画像ファイルそのものだったときは、画像として取り込む
                let item = self.import_image(bytes, None, Some(final_url), None).await;
                self.emit(
                    "add",
                    if item.is_ok() {
                        "追加しました"
                    } else {
                        "追加できませんでした"
                    },
                    1,
                    1,
                    true,
                );
                return item;
            }
            Err(e) => {
                self.emit("add", format!("追加できませんでした: {e}"), 1, 1, true);
                return Err(e);
            }
        };
        let flags = SourceFlags {
            manual: true,
            ..Default::default()
        };
        let id = self.db().upsert_item(&item, flags, &now_iso())?.id;
        self.emit("add", "追加しました", 1, 1, true);
        let detail = self.db().get_item(id)?.ok_or_else(|| anyhow!("保存に失敗しました"))?;
        Ok(detail.item)
    }

    async fn fetch_url_item(&self, raw: &str) -> Result<Fetched> {
        if let Some(id) = x_api::tweet_id_from_url(raw) {
            let item = if self.x_connected() {
                let body = self
                    .x_get(&format!("/tweets/{id}"), &x_api::single_tweet_query())
                    .await?;
                x_api::parse_single(&body)?
            } else {
                x_api::fetch_oembed(&self.http, raw, &id).await?
            };
            return Ok(Fetched::Item(Box::new(item)));
        }
        Ok(match article::fetch_resource(&self.http, raw).await? {
            article::Resource::Page(f) => Fetched::Item(Box::new(f.item)),
            article::Resource::Image { bytes, final_url } => Fetched::Image { bytes, final_url },
        })
    }

    // ------------------------------------------------------------ 画像ファイル

    pub fn images_dir(&self) -> PathBuf {
        self.data_dir.join("images")
    }

    /// 画像ファイルを取り込む。中身が同じ画像は 1 つにまとめる。
    /// name は元のファイル名、source_url は取得元（Web の画像なら）、modified_at はファイルの更新日時。
    pub async fn import_image(
        &self,
        bytes: Vec<u8>,
        name: Option<String>,
        source_url: Option<String>,
        modified_at: Option<String>,
    ) -> Result<Item> {
        let dir = self.images_dir();
        let stored = tokio::task::spawn_blocking(move || images::store(&bytes, &dir))
            .await
            .map_err(|e| anyhow!("画像の処理に失敗しました: {e}"))??;
        let url_name = source_url
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok())
            .and_then(|u| u.path_segments().and_then(|mut s| s.next_back().map(str::to_string)));
        let title = name
            .or(url_name)
            .map(|n| images::title_from_name(&n))
            .filter(|t| !t.is_empty());
        let site_name = source_url
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok())
            .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()));
        let item = NewItem {
            key: format!("image:{}", stored.hash),
            kind: KIND_IMAGE.into(),
            url: source_url.unwrap_or_default(),
            title,
            site_name,
            published_at: modified_at,
            file: Some(ImageFile {
                path: stored.file_path.display().to_string(),
                thumb_path: Some(stored.thumb_path.display().to_string()),
                width: Some(stored.width as i64),
                height: Some(stored.height as i64),
                mime: Some(stored.mime.to_string()),
                size: Some(stored.size as i64),
            }),
            ..Default::default()
        };
        let flags = SourceFlags {
            manual: true,
            ..Default::default()
        };
        let id = self.db().upsert_item(&item, flags, &now_iso())?.id;
        self.db()
            .get_item(id)?
            .map(|d| d.item)
            .ok_or_else(|| anyhow!("保存に失敗しました"))
    }

    /// パスのファイルを画像として取り込む（CLI 用）
    pub async fn import_image_file(&self, path: &std::path::Path) -> Result<Item> {
        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("ファイルを読めません: {}", path.display()))?;
        let modified = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .map(|t| DateTime::<Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        let name = path.file_name().map(|n| n.to_string_lossy().to_string());
        self.import_image(bytes, name, None, modified).await
    }

    /// アイテムを削除する。取り込んだ画像ファイルも消す（データフォルダ内のものだけ）。
    pub fn delete_item(&self, id: i64) -> Result<()> {
        let files = self.db().delete_item(id)?;
        let dir = self.images_dir();
        for f in files {
            let p = PathBuf::from(&f);
            if p.starts_with(&dir) {
                let _ = std::fs::remove_file(p);
            }
        }
        Ok(())
    }

    /// タグ付けのために画像を Claude に送れる形にする
    async fn image_for_claude(&self, item: &Item) -> Result<Option<(String, String)>> {
        let Some(file) = &item.file else { return Ok(None) };
        let path = PathBuf::from(&file.path);
        let prepared = tokio::task::spawn_blocking(move || images::for_claude(&path))
            .await
            .map_err(|e| anyhow!("画像の処理に失敗しました: {e}"))??;
        Ok(Some(prepared))
    }

    // ------------------------------------------------------------ タグ付け

    pub async fn tag_pending(&self, limit: i64) -> Result<TagReport> {
        let ids = self.db().untagged_by_ai(limit)?;
        self.tag_ids(&ids).await
    }

    pub async fn tag_ids(&self, ids: &[i64]) -> Result<TagReport> {
        let Ok(_guard) = self.tag_lock.try_lock() else {
            bail!("すでにタグ付け中です");
        };
        let key = self
            .anthropic_key()
            .ok_or_else(|| anyhow!("Anthropic の API キーが未設定です。設定画面で入力してください"))?;
        let model = self.claude_model();
        let total = ids.len() as i64;
        let mut report = TagReport::default();
        let mut consecutive_failures = 0;
        for (i, &id) in ids.iter().enumerate() {
            self.emit(
                "tag",
                format!("Claude でタグ付け中…（{}/{}）", i + 1, total),
                i as i64,
                total,
                false,
            );
            let (item, vocab) = {
                let db = self.db();
                (db.item_for_tagging(id)?, db.list_tags()?)
            };
            let Some((item, linked_text)) = item else { continue };
            let result = match self.image_for_claude(&item).await {
                Ok(image) => {
                    let image = image
                        .as_ref()
                        .map(|(media_type, data)| tagger::ImageInput { media_type, data });
                    tagger::tag_item(&self.http, &key, &model, &item, linked_text.as_deref(), image, &vocab).await
                }
                Err(e) => Err(e),
            };
            match result {
                Ok(r) => {
                    let mut db = self.db();
                    db.add_item_tags(id, &r.tags, "ai")?;
                    if !r.summary.is_empty() {
                        db.set_summary(id, &r.summary)?;
                    }
                    if let (Some(text), true) = (&r.text, item.kind == KIND_IMAGE) {
                        db.set_text(id, text)?;
                    }
                    db.mark_ai_tagged(id)?;
                    report.tagged += 1;
                    consecutive_failures = 0;
                }
                Err(e) if e.downcast_ref::<tagger::Refused>().is_some() => {
                    // 再試行しても同じなので、処理済みにして先へ進む
                    self.db().mark_ai_tagged(id)?;
                    report.failed += 1;
                    report.errors.push(format!("#{id}: {e}"));
                }
                Err(e) => {
                    report.failed += 1;
                    report.errors.push(format!("#{id}: {e:#}"));
                    consecutive_failures += 1;
                    if consecutive_failures >= 3 {
                        report.errors.push("失敗が続いたため中断しました".into());
                        break;
                    }
                }
            }
        }
        let msg = if report.failed > 0 {
            format!("{} 件をタグ付け（失敗 {} 件）", report.tagged, report.failed)
        } else {
            format!("{} 件をタグ付けしました", report.tagged)
        };
        self.emit("tag", msg, total, total, true);
        Ok(report)
    }

    /// 1 件を Claude でタグ付けし直す（AI が付けたタグは入れ替え、手動のタグは残す）
    pub async fn retag_item(&self, id: i64) -> Result<ItemDetail> {
        let key = self
            .anthropic_key()
            .ok_or_else(|| anyhow!("Anthropic の API キーが未設定です"))?;
        let model = self.claude_model();
        let (item, vocab) = {
            let db = self.db();
            (db.item_for_tagging(id)?, db.list_tags()?)
        };
        let (item, linked_text) = item.ok_or_else(|| anyhow!("アイテムが見つかりません"))?;
        let image = self.image_for_claude(&item).await?;
        let image = image
            .as_ref()
            .map(|(media_type, data)| tagger::ImageInput { media_type, data });
        let r = tagger::tag_item(&self.http, &key, &model, &item, linked_text.as_deref(), image, &vocab).await?;
        {
            let mut db = self.db();
            db.remove_ai_tags(id)?;
            db.add_item_tags(id, &r.tags, "ai")?;
            if !r.summary.is_empty() {
                db.set_summary(id, &r.summary)?;
            }
            if let (Some(text), true) = (&r.text, item.kind == KIND_IMAGE) {
                db.set_text(id, text)?;
            }
            db.mark_ai_tagged(id)?;
        }
        self.db()
            .get_item(id)?
            .ok_or_else(|| anyhow!("アイテムが見つかりません"))
    }

    // ------------------------------------------------------------ 紹介先ページの保存

    /// ポストが紹介しているリンク先ページを取得し、本文ごと保存する。
    /// ids が None なら、まだ保存していないもの（retry_errors なら前回失敗したものも）をまとめて処理する。
    pub async fn fetch_linked_pages(&self, ids: Option<&[i64]>, retry_errors: bool) -> Result<i64> {
        let Ok(_guard) = self.link_lock.try_lock() else {
            bail!("すでに紹介先ページを保存中です");
        };
        let targets = self.db().items_needing_link(ids, retry_errors, 5000)?;
        if targets.is_empty() {
            return Ok(0);
        }
        let total = targets.len() as i64;
        let mut saved = 0;
        let mut done = 0i64;
        let mut set = tokio::task::JoinSet::new();
        let mut queue = targets.into_iter();
        loop {
            while set.len() < 4 {
                let Some((id, url)) = queue.next() else { break };
                let http = self.http.clone();
                set.spawn(async move {
                    let r = article::fetch(&http, &url).await;
                    (id, url, r)
                });
            }
            let Some(joined) = set.join_next().await else { break };
            done += 1;
            if let Ok((id, url, result)) = joined {
                let stored = match &result {
                    Ok(f) => self.db().save_linked_page(id, &f.final_url, Ok(&f.item)),
                    Err(e) => self.db().save_linked_page(id, &url, Err(&format!("{e:#}"))),
                };
                if stored.is_ok() && result.is_ok() {
                    saved += 1;
                }
            }
            self.emit(
                "link",
                format!("紹介先のページを保存中…（{done}/{total}）"),
                done,
                total,
                done == total,
            );
        }
        if saved < total {
            self.emit(
                "link",
                format!(
                    "紹介先のページを {saved} 件保存（取得できなかったもの {} 件）",
                    total - saved
                ),
                total,
                total,
                true,
            );
        }
        Ok(saved)
    }

    /// 1 件の紹介先ページを取り直す
    pub async fn refetch_linked_page(&self, id: i64) -> Result<ItemDetail> {
        let url = self
            .db()
            .get_item(id)?
            .and_then(|d| d.item.link.map(|l| l.url))
            .ok_or_else(|| anyhow!("このアイテムには紹介先のリンクがありません"))?;
        let result = article::fetch(&self.http, &url).await;
        match &result {
            Ok(f) => self.db().save_linked_page(id, &f.final_url, Ok(&f.item))?,
            Err(e) => self.db().save_linked_page(id, &url, Err(&format!("{e:#}")))?,
        }
        if let Err(e) = result {
            return Err(e.context("紹介先のページを取得できませんでした"));
        }
        let _ = self.download_media(Some(&[id])).await;
        self.db()
            .get_item(id)?
            .ok_or_else(|| anyhow!("アイテムが見つかりません"))
    }

    // ------------------------------------------------------------ 画像の保存

    pub async fn download_media(&self, ids: Option<&[i64]>) -> Result<i64> {
        if !self.get_bool(keys::DOWNLOAD_MEDIA, true) {
            return Ok(0);
        }
        let urls = self.db().media_urls_to_download(ids, 3000)?;
        if urls.is_empty() {
            return Ok(0);
        }
        let total = urls.len() as i64;
        let mut saved = 0;
        let mut set = tokio::task::JoinSet::new();
        let mut queue = urls.into_iter();
        let mut done = 0i64;
        loop {
            while set.len() < 4 {
                let Some(url) = queue.next() else { break };
                let http = self.http.clone();
                let dir = self.media_dir.clone();
                set.spawn(async move {
                    let r = fetch_media(&http, &url, &dir).await;
                    (url, r)
                });
            }
            let Some(joined) = set.join_next().await else { break };
            done += 1;
            if let Ok((url, Ok((path, bytes)))) = joined {
                if self.db().record_media_file(&url, &path, bytes).is_ok() {
                    saved += 1;
                }
            }
            if done % 5 == 0 || done == total {
                self.emit(
                    "media",
                    format!("画像を保存中…（{done}/{total}）"),
                    done,
                    total,
                    done == total,
                );
            }
        }
        Ok(saved)
    }
}

fn media_extension(content_type: &str, url: &str) -> &'static str {
    match content_type.split(';').next().unwrap_or("").trim() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/avif" => "avif",
        "image/svg+xml" => "svg",
        _ => {
            let path = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
            ["jpg", "jpeg", "png", "gif", "webp", "avif"]
                .into_iter()
                .find(|e| path.ends_with(&format!(".{e}")))
                .map(|e| if e == "jpeg" { "jpg" } else { e })
                .unwrap_or("img")
        }
    }
}

async fn fetch_media(http: &reqwest::Client, url: &str, dir: &std::path::Path) -> Result<(String, i64)> {
    let resp = http.get(url).timeout(Duration::from_secs(30)).send().await?;
    if !resp.status().is_success() {
        bail!("{}", resp.status());
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !ct.is_empty() && !ct.starts_with("image/") {
        bail!("画像ではありません: {ct}");
    }
    let bytes = resp.bytes().await?;
    if bytes.len() > MAX_MEDIA_BYTES {
        bail!("大きすぎます");
    }
    let hash = Sha256::digest(url.as_bytes());
    let name: String = hash.iter().take(12).map(|b| format!("{b:02x}")).collect();
    let path = dir.join(format!("{name}.{}", media_extension(&ct, url)));
    tokio::fs::write(&path, &bytes).await?;
    Ok((path.display().to_string(), bytes.len() as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core() -> (Core, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let c = Core::new(dir.path().to_path_buf(), Box::new(|_| {})).unwrap();
        (c, dir)
    }

    #[test]
    fn settings_roundtrip_hides_secrets() {
        let (c, _d) = core();
        let s = c
            .save_settings(SettingsPatch {
                x_client_id: Some(" CID ".into()),
                x_client_secret: Some("SECRET".into()),
                anthropic_api_key: Some("sk-ant-xxx".into()),
                pickup_count: Some(100),
                auto_sync: Some(false),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.x_client_id, "CID");
        assert!(s.has_x_client_secret && s.has_anthropic_key);
        assert_eq!(s.pickup_count, 50, "上限で丸める");
        assert!(!s.auto_sync);
        assert_eq!(s.claude_model, "claude-opus-5-5");
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("SECRET") && !json.contains("sk-ant-xxx"),
            "秘密情報は画面に渡さない"
        );

        let s = c
            .save_settings(SettingsPatch {
                x_client_secret: Some("".into()),
                ..Default::default()
            })
            .unwrap();
        assert!(!s.has_x_client_secret, "空文字で削除");
    }

    #[test]
    fn auto_sync_needs_connection_and_interval() {
        let (c, _d) = core();
        assert!(!c.should_auto_sync(), "未連携なら同期しない");
        c.set(keys::X_REFRESH, "r").unwrap();
        assert!(c.should_auto_sync(), "一度も同期していなければ同期する");
        c.set(keys::LAST_SYNC_AT, &now_iso()).unwrap();
        assert!(!c.should_auto_sync(), "直近に同期済みなら同期しない");
        let old = (Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
        c.set(keys::LAST_SYNC_AT, &old).unwrap();
        assert!(c.should_auto_sync());
    }

    /// ローカルに立てた HTTP サーバーから紹介先ページを取得して保存する
    #[tokio::test]
    async fn fetches_and_stores_linked_page_end_to_end() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, body) = match path.as_str() {
                    // 短縮 URL のようにリダイレクトする
                    "/short" => ("301 Moved Permanently\r\nLocation: /article", String::new()),
                    "/article" => (
                        "200 OK\r\nContent-Type: text/html; charset=utf-8",
                        "<html><head><title>紹介された記事</title><meta property=\"og:site_name\" content=\"ローカルブログ\"></head>\
                         <body><article><h1>紹介された記事</h1><p>ポストが紹介していた記事の本文です。ローカルに保存しておけば、\
                         元のページが消えても読み返せます。trigram による全文検索の対象にもなります。</p>\
                         <p>二つ目の段落です。十分な長さの本文があると Readability が本文として認識します。</p></article></body></html>"
                            .to_string(),
                    ),
                    _ => ("404 Not Found", String::new()),
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });

        let (c, _d) = core();
        let mut t = NewItem {
            key: "tweet:1".into(),
            kind: KIND_TWEET.into(),
            url: "https://x.com/a/status/1".into(),
            text: "おすすめ".into(),
            ..Default::default()
        };
        t.link = Some(LinkCard {
            url: format!("http://127.0.0.1:{port}/short"),
            ..Default::default()
        });
        let ok_id = c.db().upsert_item(&t, SourceFlags::default(), &now_iso()).unwrap().id;
        t.key = "tweet:2".into();
        t.link = Some(LinkCard {
            url: format!("http://127.0.0.1:{port}/missing"),
            ..Default::default()
        });
        let ng_id = c.db().upsert_item(&t, SourceFlags::default(), &now_iso()).unwrap().id;

        let saved = c.fetch_linked_pages(None, false).await.unwrap();
        assert_eq!(saved, 1);

        let detail = c.db().get_item(ok_id).unwrap().unwrap();
        let page = detail.item.linked.unwrap();
        assert_eq!(
            page.url,
            format!("http://127.0.0.1:{port}/article"),
            "リダイレクト後の URL"
        );
        assert_eq!(page.site_name.as_deref(), Some("ローカルブログ"));
        assert!(detail.linked_text.unwrap().contains("元のページが消えても読み返せます"));
        assert!(detail.linked_html.unwrap().contains("<p>"));

        let failed = c.db().get_item(ng_id).unwrap().unwrap().item.linked.unwrap();
        assert!(failed.error.unwrap().contains("404"));

        let hits = c
            .db()
            .list_items(&ItemQuery {
                keyword: Some("読み返せます".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(hits.items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![ok_id]);

        // 失敗したものは retry で取り直しの対象になる（まだ 404 なので失敗のまま）
        assert_eq!(c.fetch_linked_pages(None, true).await.unwrap(), 0);
        assert!(c.refetch_linked_page(ng_id).await.is_err());
        assert!(c.refetch_linked_page(ok_id).await.is_ok());
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[tokio::test]
    async fn imports_images_dedups_and_deletes_files() {
        let (c, _d) = core();
        let bytes = png_bytes(1200, 800);
        let item = c
            .import_image(
                bytes.clone(),
                Some("レシピ メモ.png".into()),
                None,
                Some("2026-09-01T10:00:00Z".into()),
            )
            .await
            .unwrap();
        assert_eq!(item.kind, "image");
        assert_eq!(item.title.as_deref(), Some("レシピ メモ"));
        assert!(item.is_manual);
        let file = item.file.clone().unwrap();
        assert_eq!(
            (file.width, file.height, file.mime.as_deref()),
            (Some(1200), Some(800), Some("image/png"))
        );
        assert!(std::path::Path::new(&file.path).exists());
        assert!(std::path::Path::new(file.thumb_path.as_ref().unwrap()).exists());

        // 同じ画像を別名で取り込んでも 1 件にまとまる
        let again = c
            .import_image(bytes, Some("copy.png".into()), None, None)
            .await
            .unwrap();
        assert_eq!(again.id, item.id);
        let s = c.db().stats().unwrap();
        assert_eq!((s.total, s.images), (1, 1));
        let listed = c
            .db()
            .list_items(&ItemQuery {
                filter: Some("image".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(listed.total, 1);

        // 書き起こした文字で検索できる
        c.db().set_text(item.id, "材料 玉ねぎ にんじん").unwrap();
        let hits = c
            .db()
            .list_items(&ItemQuery {
                keyword: Some("にんじん".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(hits.total, 1);

        // 形式が違うものは取り込まない
        assert!(c
            .import_image(b"GIF89a-broken".to_vec(), None, None, None)
            .await
            .is_err());
        assert!(c.import_image(b"hello".to_vec(), None, None, None).await.is_err());

        c.delete_item(item.id).unwrap();
        assert!(
            !std::path::Path::new(&file.path).exists(),
            "削除すると画像ファイルも消える"
        );
        assert!(!std::path::Path::new(file.thumb_path.as_ref().unwrap()).exists());
    }

    /// 画像の URL を追加すると、画像として取り込まれる
    #[tokio::test]
    async fn adding_an_image_url_imports_the_image() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let body = png_bytes(64, 48);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = body.clone();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = vec![0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    served.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&served).await;
            }
        });
        let (c, _d) = core();
        let url = format!("http://127.0.0.1:{port}/photos/cat.png?utm_source=x");
        let item = c.add_url(&url).await.unwrap();
        assert_eq!(item.kind, "image");
        assert_eq!(item.title.as_deref(), Some("cat"));
        assert_eq!(item.site_name.as_deref(), Some("127.0.0.1"));
        assert!(item.url.starts_with("http://127.0.0.1"));
        assert_eq!(item.file.unwrap().width, Some(64));
    }

    #[test]
    fn media_extension_from_type_or_url() {
        assert_eq!(media_extension("image/jpeg", "x"), "jpg");
        assert_eq!(media_extension("", "https://a/b.PNG?x=1"), "png");
        assert_eq!(media_extension("application/octet-stream", "https://a/b"), "img");
    }
}
