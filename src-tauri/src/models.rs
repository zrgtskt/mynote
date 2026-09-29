use serde::{Deserialize, Serialize};

pub const KIND_TWEET: &str = "tweet";
pub const KIND_ARTICLE: &str = "article";

/// ツイートに添付された画像・動画
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    /// photo / video / animated_gif
    pub kind: String,
    pub url: Option<String>,
    pub preview_url: Option<String>,
    pub video_url: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub alt: Option<String>,
    /// ローカルに保存済みなら、そのファイルパス（表示時に付与）
    #[serde(default, skip_deserializing)]
    pub local_path: Option<String>,
}

impl Media {
    /// 表示・保存に使う画像 URL（動画はサムネイル）
    pub fn image_url(&self) -> Option<&str> {
        self.url.as_deref().or(self.preview_url.as_deref())
    }
}

/// ツイート内リンクのプレビュー
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LinkCard {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
}

/// 保存前のアイテム（ツイート・記事共通）
#[derive(Clone, Debug, Default)]
pub struct NewItem {
    /// 重複判定用のキー（tweet:<id> / url:<正規化URL>）
    pub key: String,
    pub kind: String,
    pub url: String,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub author_name: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar: Option<String>,
    pub site_name: Option<String>,
    pub text: String,
    pub content_html: Option<String>,
    pub excerpt: Option<String>,
    pub image_url: Option<String>,
    pub media: Vec<Media>,
    pub link: Option<LinkCard>,
    pub metrics: Option<serde_json::Value>,
    pub lang: Option<String>,
    pub published_at: Option<String>,
    pub raw_json: Option<String>,
}

/// どこから保存されたか
#[derive(Clone, Copy, Debug, Default)]
pub struct SourceFlags {
    pub liked: bool,
    pub bookmarked: bool,
    pub manual: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub kind: String,
    pub url: String,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub author_name: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar: Option<String>,
    pub author_avatar_local: Option<String>,
    pub site_name: Option<String>,
    pub text: String,
    pub excerpt: Option<String>,
    pub summary: Option<String>,
    pub image_url: Option<String>,
    pub image_local: Option<String>,
    pub media: Vec<Media>,
    pub link: Option<LinkCard>,
    pub metrics: Option<serde_json::Value>,
    pub published_at: Option<String>,
    pub saved_at: String,
    pub is_liked: bool,
    pub is_bookmarked: bool,
    pub is_manual: bool,
    pub ai_tagged_at: Option<String>,
    pub note: Option<String>,
    pub has_content: bool,
    pub tags: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    #[serde(flatten)]
    pub item: Item,
    pub content_html: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ItemQuery {
    pub keyword: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// "and" / "or"
    pub tag_mode: Option<String>,
    /// all / liked / bookmarked / tweet / article / untagged / manual
    pub filter: Option<String>,
    /// saved_desc / saved_asc / published_desc / random
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ItemPage {
    pub items: Vec<Item>,
    pub total: i64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub name: String,
    pub count: i64,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total: i64,
    pub liked: i64,
    pub bookmarked: i64,
    pub tweets: i64,
    pub articles: i64,
    pub manual: i64,
    pub untagged: i64,
    pub tags: i64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DailyPickup {
    pub day: String,
    pub items: Vec<Item>,
}

/// 画面に渡す設定（秘密情報は伏せる）
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PublicSettings {
    pub x_client_id: String,
    pub has_x_client_secret: bool,
    pub x_redirect_uri: String,
    pub x_connected: bool,
    pub x_username: Option<String>,
    pub x_name: Option<String>,
    pub x_avatar: Option<String>,
    pub sync_likes: bool,
    pub sync_bookmarks: bool,
    pub sync_max_items: i64,
    pub auto_sync: bool,
    pub last_sync_at: Option<String>,
    pub last_sync_summary: Option<String>,
    pub has_anthropic_key: bool,
    pub anthropic_key_from_env: bool,
    pub claude_model: String,
    pub auto_tag: bool,
    pub pickup_count: i64,
    pub download_media: bool,
    pub data_dir: String,
}

/// 設定の部分更新（None の項目は変更しない）
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub x_client_id: Option<String>,
    /// 空文字で削除
    pub x_client_secret: Option<String>,
    pub sync_likes: Option<bool>,
    pub sync_bookmarks: Option<bool>,
    pub sync_max_items: Option<i64>,
    pub auto_sync: Option<bool>,
    /// 空文字で削除
    pub anthropic_api_key: Option<String>,
    pub claude_model: Option<String>,
    pub auto_tag: Option<bool>,
    pub pickup_count: Option<i64>,
    pub download_media: Option<bool>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// sync / tag / media / add
    pub task: String,
    pub message: String,
    pub current: i64,
    pub total: i64,
    pub done: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub liked_fetched: i64,
    pub liked_new: i64,
    pub bookmarks_fetched: i64,
    pub bookmarks_new: i64,
    pub tagged: i64,
    pub media_saved: i64,
    pub errors: Vec<String>,
}

impl SyncReport {
    pub fn summary(&self) -> String {
        let mut s = format!(
            "いいね {} 件（新規 {}）・ブックマーク {} 件（新規 {}）を取得",
            self.liked_fetched, self.liked_new, self.bookmarks_fetched, self.bookmarks_new
        );
        if self.tagged > 0 {
            s.push_str(&format!("、{} 件をタグ付け", self.tagged));
        }
        if !self.errors.is_empty() {
            s.push_str(&format!("（エラー {} 件）", self.errors.len()));
        }
        s
    }
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TagReport {
    pub tagged: i64,
    pub failed: i64,
    pub errors: Vec<String>,
}
