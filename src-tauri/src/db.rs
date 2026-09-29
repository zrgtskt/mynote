use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use crate::models::*;

const SCHEMA_VERSION: i64 = 2;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS items (
    id            INTEGER PRIMARY KEY,
    key           TEXT NOT NULL UNIQUE,
    kind          TEXT NOT NULL,
    url           TEXT NOT NULL,
    external_id   TEXT,
    title         TEXT,
    author_name   TEXT,
    author_handle TEXT,
    author_avatar TEXT,
    site_name     TEXT,
    text          TEXT NOT NULL DEFAULT '',
    content_html  TEXT,
    excerpt       TEXT,
    summary       TEXT,
    image_url     TEXT,
    media_json    TEXT,
    link_json     TEXT,
    metrics_json  TEXT,
    lang          TEXT,
    published_at  TEXT,
    saved_at      TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    is_liked      INTEGER NOT NULL DEFAULT 0,
    is_bookmarked INTEGER NOT NULL DEFAULT 0,
    is_manual     INTEGER NOT NULL DEFAULT 0,
    ai_tagged_at  TEXT,
    note          TEXT,
    raw_json      TEXT
);
CREATE INDEX IF NOT EXISTS idx_items_saved_at ON items(saved_at);
CREATE INDEX IF NOT EXISTS idx_items_published_at ON items(published_at);

CREATE TABLE IF NOT EXISTS tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE IF NOT EXISTS item_tags (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    source  TEXT NOT NULL DEFAULT 'manual',
    PRIMARY KEY (item_id, tag_id)
);
CREATE INDEX IF NOT EXISTS idx_item_tags_tag ON item_tags(tag_id);

-- 日本語でも部分一致できるよう trigram で索引する
CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
    title, body, author, note, summary,
    tokenize = 'trigram'
);

CREATE TABLE IF NOT EXISTS daily_picks (
    day      TEXT NOT NULL,
    item_id  INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    PRIMARY KEY (day, item_id)
);

CREATE TABLE IF NOT EXISTS media_files (
    url        TEXT PRIMARY KEY,
    path       TEXT NOT NULL,
    bytes      INTEGER NOT NULL,
    fetched_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS kv (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// v2: ポストが紹介しているリンク先ページを本文ごと保存する
const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS linked_pages (
    item_id      INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
    url          TEXT NOT NULL,
    title        TEXT,
    site_name    TEXT,
    author       TEXT,
    excerpt      TEXT,
    image_url    TEXT,
    published_at TEXT,
    content_html TEXT,
    text         TEXT,
    fetched_at   TEXT NOT NULL,
    error        TEXT
);
"#;

/// 一覧・詳細で共通の SELECT 列。text は一覧では切り詰める。
const ITEM_COLUMNS: &str = "id, kind, url, external_id, title, author_name, author_handle, author_avatar, \
     site_name, {TEXT}, excerpt, summary, image_url, media_json, link_json, metrics_json, published_at, \
     saved_at, is_liked, is_bookmarked, is_manual, ai_tagged_at, note, content_html IS NOT NULL AND content_html <> ''";

/// 一覧で返す本文の最大文字数
const LIST_TEXT_CHARS: i64 = 600;

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub struct UpsertResult {
    pub id: i64,
    /// 今回のフラグ（いいね等）が新たに立ったか
    pub newly_flagged: bool,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("DB を開けません: {}", path.display()))?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
        }
        if version < 2 {
            conn.execute_batch(SCHEMA_V2)?;
        }
        if version < SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { conn })
    }

    // ---------------------------------------------------------------- kv

    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn kv_delete(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM kv WHERE key = ?1", [key])?;
        Ok(())
    }

    // ---------------------------------------------------------------- items

    /// ツイート・記事を保存する。既にあれば内容を更新し、フラグは OR で足し合わせる。
    pub fn upsert_item(&mut self, item: &NewItem, flags: SourceFlags, saved_at: &str) -> Result<UpsertResult> {
        let tx = self.conn.transaction()?;
        let res = upsert_item_tx(&tx, item, flags, saved_at)?;
        tx.commit()?;
        Ok(res)
    }

    /// 複数件をまとめて保存する（1 トランザクション）
    pub fn upsert_items(&mut self, items: &[(NewItem, String)], flags: SourceFlags) -> Result<Vec<UpsertResult>> {
        let tx = self.conn.transaction()?;
        let mut out = Vec::with_capacity(items.len());
        for (item, saved_at) in items {
            out.push(upsert_item_tx(&tx, item, flags, saved_at)?);
        }
        tx.commit()?;
        Ok(out)
    }

    pub fn get_item(&self, id: i64) -> Result<Option<ItemDetail>> {
        let sql = format!("SELECT {} , content_html FROM items WHERE id = ?1", item_columns(false));
        let row = self
            .conn
            .query_row(&sql, [id], |r| {
                let item = row_to_item(r)?;
                let html: Option<String> = r.get(24)?;
                Ok((item, html))
            })
            .optional()?;
        let Some((item, content_html)) = row else {
            return Ok(None);
        };
        let mut items = vec![item];
        self.attach_extras(&mut items)?;
        let (linked_html, linked_text) = self
            .conn
            .query_row(
                "SELECT content_html, text FROM linked_pages WHERE item_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or((None, None));
        Ok(Some(ItemDetail {
            item: items.pop().unwrap(),
            content_html,
            linked_html,
            linked_text,
        }))
    }

    pub fn delete_item(&mut self, id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM items_fts WHERE rowid = ?1", [id])?;
        tx.execute("DELETE FROM items WHERE id = ?1", [id])?;
        delete_orphan_tags(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_note(&mut self, id: i64, note: &str) -> Result<()> {
        let note = note.trim();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE items SET note = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, if note.is_empty() { None } else { Some(note) }, now_iso()],
        )?;
        reindex_tx(&tx, id)?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_summary(&mut self, id: i64, summary: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE items SET summary = ?2 WHERE id = ?1",
            params![id, summary.trim()],
        )?;
        reindex_tx(&tx, id)?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_ai_tagged(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE items SET ai_tagged_at = ?2 WHERE id = ?1",
            params![id, now_iso()],
        )?;
        Ok(())
    }

    /// AI タグ付けがまだのアイテム
    pub fn untagged_by_ai(&self, limit: i64) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM items WHERE ai_tagged_at IS NULL ORDER BY saved_at DESC LIMIT ?1")?;
        let ids = stmt
            .query_map([limit], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
        Ok(ids)
    }

    pub fn count_untagged_by_ai(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM items WHERE ai_tagged_at IS NULL", [], |r| {
                r.get(0)
            })?)
    }

    pub fn list_items(&self, q: &ItemQuery) -> Result<ItemPage> {
        let (where_sql, args) = build_where(q);
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM items i WHERE {where_sql}"),
            params_from_iter(args.iter()),
            |r| r.get(0),
        )?;

        let order = match q.sort.as_deref() {
            Some("saved_asc") => "i.saved_at ASC, i.id ASC",
            Some("published_desc") => "COALESCE(i.published_at, i.saved_at) DESC, i.id DESC",
            Some("random") => "RANDOM()",
            _ => "i.saved_at DESC, i.id DESC",
        };
        let limit = q.limit.unwrap_or(60).clamp(1, 500);
        let offset = q.offset.unwrap_or(0).max(0);
        let sql = format!(
            "SELECT {} FROM items i WHERE {where_sql} ORDER BY {order} LIMIT {limit} OFFSET {offset}",
            item_columns(true)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut items = stmt
            .query_map(params_from_iter(args.iter()), row_to_item)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.attach_extras(&mut items)?;
        Ok(ItemPage { items, total })
    }

    fn items_by_ids(&self, ids: &[i64]) -> Result<Vec<Item>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!("SELECT {} FROM items WHERE id IN ({placeholders})", item_columns(true));
        let mut stmt = self.conn.prepare(&sql)?;
        let mut items = stmt
            .query_map(params_from_iter(ids.iter()), row_to_item)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // 渡された順に並べ直す
        let pos: HashMap<i64, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        items.sort_by_key(|it| pos.get(&it.id).copied().unwrap_or(usize::MAX));
        self.attach_extras(&mut items)?;
        Ok(items)
    }

    /// タグとローカル保存済みメディアのパスを付ける
    fn attach_extras(&self, items: &mut [Item]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let ids: Vec<i64> = items.iter().map(|i| i.id).collect();
        let placeholders = vec!["?"; ids.len()].join(",");
        let mut tags: HashMap<i64, Vec<String>> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT it.item_id, t.name FROM item_tags it JOIN tags t ON t.id = it.tag_id \
                 WHERE it.item_id IN ({placeholders}) ORDER BY t.name COLLATE NOCASE"
            ))?;
            let rows = stmt.query_map(params_from_iter(ids.iter()), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, name) = row?;
                tags.entry(id).or_default().push(name);
            }
        }

        let mut linked: HashMap<i64, LinkedPage> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT item_id, url, title, site_name, author, excerpt, image_url, published_at, fetched_at, error, \
                        content_html IS NOT NULL AND content_html <> '' \
                 FROM linked_pages WHERE item_id IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(params_from_iter(ids.iter()), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    LinkedPage {
                        url: r.get(1)?,
                        title: r.get(2)?,
                        site_name: r.get(3)?,
                        author: r.get(4)?,
                        excerpt: r.get(5)?,
                        image_url: r.get(6)?,
                        image_local: None,
                        published_at: r.get(7)?,
                        fetched_at: r.get(8)?,
                        error: r.get(9)?,
                        has_content: r.get::<_, i64>(10)? != 0,
                    },
                ))
            })?;
            for row in rows {
                let (id, page) = row?;
                linked.insert(id, page);
            }
        }

        let mut urls: Vec<String> = Vec::new();
        for it in items.iter() {
            urls.extend(it.author_avatar.clone());
            urls.extend(it.image_url.clone());
            for m in &it.media {
                urls.extend(m.image_url().map(str::to_string));
            }
            urls.extend(it.link.as_ref().and_then(|l| l.image.clone()));
        }
        urls.extend(linked.values().filter_map(|p| p.image_url.clone()));
        let local = self.local_media_paths(&urls)?;

        for it in items.iter_mut() {
            it.tags = tags.remove(&it.id).unwrap_or_default();
            it.linked = linked.remove(&it.id).map(|mut p| {
                p.image_local = p.image_url.as_ref().and_then(|u| local.get(u).cloned());
                p
            });
            if let Some(link) = it.link.as_mut() {
                link.image_local = link.image.as_ref().and_then(|u| local.get(u).cloned());
            }
            it.author_avatar_local = it.author_avatar.as_ref().and_then(|u| local.get(u).cloned());
            it.image_local = it.image_url.as_ref().and_then(|u| local.get(u).cloned());
            for m in it.media.iter_mut() {
                m.local_path = m.image_url().and_then(|u| local.get(u).cloned());
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------- tags

    pub fn list_tags(&self) -> Result<Vec<TagCount>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name, COUNT(it.item_id) AS c FROM tags t LEFT JOIN item_tags it ON it.tag_id = t.id \
             GROUP BY t.id HAVING c > 0 ORDER BY c DESC, t.name COLLATE NOCASE",
        )?;
        let tags = stmt
            .query_map([], |r| {
                Ok(TagCount {
                    name: r.get(0)?,
                    count: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(tags)
    }

    /// アイテムのタグを丸ごと置き換える
    pub fn set_item_tags(&mut self, item_id: i64, tags: &[String], source: &str) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM item_tags WHERE item_id = ?1", [item_id])?;
        for name in normalize_tags(tags) {
            let tag_id = ensure_tag(&tx, &name)?;
            tx.execute(
                "INSERT OR IGNORE INTO item_tags (item_id, tag_id, source) VALUES (?1, ?2, ?3)",
                params![item_id, tag_id, source],
            )?;
        }
        delete_orphan_tags(&tx)?;
        tx.commit()?;
        self.item_tags(item_id)
    }

    /// 既存タグを残したままタグを追加する（AI タグ付け用）
    pub fn add_item_tags(&mut self, item_id: i64, tags: &[String], source: &str) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        for name in normalize_tags(tags) {
            let tag_id = ensure_tag(&tx, &name)?;
            tx.execute(
                "INSERT OR IGNORE INTO item_tags (item_id, tag_id, source) VALUES (?1, ?2, ?3)",
                params![item_id, tag_id, source],
            )?;
        }
        tx.commit()?;
        self.item_tags(item_id)
    }

    /// AI が付けたタグだけを外す（手動で付けたタグは残す）
    pub fn remove_ai_tags(&mut self, item_id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM item_tags WHERE item_id = ?1 AND source = 'ai'", [item_id])?;
        delete_orphan_tags(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn item_tags(&self, item_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name FROM item_tags it JOIN tags t ON t.id = it.tag_id WHERE it.item_id = ?1 ORDER BY t.name COLLATE NOCASE",
        )?;
        let tags = stmt
            .query_map([item_id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(tags)
    }

    /// タグ名を変更する。変更先が既にあれば統合する。
    pub fn rename_tag(&mut self, from: &str, to: &str) -> Result<()> {
        let to = normalize_tag(to).context("新しいタグ名が空です")?;
        let tx = self.conn.transaction()?;
        let from_id: Option<i64> = tx
            .query_row("SELECT id FROM tags WHERE name = ?1", [from], |r| r.get(0))
            .optional()?;
        let Some(from_id) = from_id else {
            anyhow::bail!("タグ「{from}」が見つかりません");
        };
        let to_id: Option<i64> = tx
            .query_row("SELECT id FROM tags WHERE name = ?1", [&to], |r| r.get(0))
            .optional()?;
        match to_id {
            Some(to_id) if to_id != from_id => {
                tx.execute(
                    "INSERT OR IGNORE INTO item_tags (item_id, tag_id, source) SELECT item_id, ?2, source FROM item_tags WHERE tag_id = ?1",
                    params![from_id, to_id],
                )?;
                tx.execute("DELETE FROM tags WHERE id = ?1", [from_id])?;
            }
            _ => {
                // 同じタグ（大文字小文字だけの変更を含む）
                tx.execute("UPDATE tags SET name = ?2 WHERE id = ?1", params![from_id, to])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_tag(&mut self, name: &str) -> Result<()> {
        self.conn.execute("DELETE FROM tags WHERE name = ?1", [name])?;
        Ok(())
    }

    // ---------------------------------------------------------------- stats

    pub fn stats(&self) -> Result<Stats> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*), \
                COALESCE(SUM(is_liked), 0), COALESCE(SUM(is_bookmarked), 0), \
                COALESCE(SUM(kind = 'tweet'), 0), COALESCE(SUM(kind = 'article'), 0), \
                COALESCE(SUM(is_manual), 0), \
                COALESCE(SUM(NOT EXISTS (SELECT 1 FROM item_tags it WHERE it.item_id = items.id)), 0), \
                (SELECT COUNT(*) FROM tags) \
             FROM items",
            [],
            |r| {
                Ok(Stats {
                    total: r.get(0)?,
                    liked: r.get(1)?,
                    bookmarked: r.get(2)?,
                    tweets: r.get(3)?,
                    articles: r.get(4)?,
                    manual: r.get(5)?,
                    untagged: r.get(6)?,
                    tags: r.get(7)?,
                })
            },
        )?)
    }

    // ---------------------------------------------------------------- daily pickup

    /// その日のピックアップ。一度選んだらその日は固定し、直近 30 日に出たものはなるべく避ける。
    pub fn daily_pickup(&mut self, day: &str, count: i64, reshuffle: bool) -> Result<DailyPickup> {
        let count = count.clamp(1, 50) as usize;
        let tx = self.conn.transaction()?;
        if reshuffle {
            tx.execute("DELETE FROM daily_picks WHERE day = ?1", [day])?;
        }
        let mut picked: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT item_id FROM daily_picks WHERE day = ?1 ORDER BY position")?;
            let ids = stmt
                .query_map([day], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()?;
            ids
        };
        if picked.len() < count {
            let need = count - picked.len();
            let fresh: Vec<i64> = {
                let mut stmt = tx.prepare(
                    "SELECT id FROM items WHERE id NOT IN (SELECT item_id FROM daily_picks WHERE day >= date(?1, '-30 days'))",
                )?;
                let ids = stmt
                    .query_map([day], |r| r.get(0))?
                    .collect::<rusqlite::Result<Vec<i64>>>()?;
                ids
            };
            let mut rng = rand::rng();
            let mut chosen = choose_random(&fresh, need, &mut rng);
            if chosen.len() < need {
                // 候補が足りなければ、今日まだ出ていないもの全体から補う
                let rest: Vec<i64> = {
                    let mut stmt = tx.prepare(
                        "SELECT id FROM items WHERE id NOT IN (SELECT item_id FROM daily_picks WHERE day = ?1)",
                    )?;
                    let ids = stmt
                        .query_map([day], |r| r.get(0))?
                        .collect::<rusqlite::Result<Vec<i64>>>()?;
                    ids.into_iter().filter(|id| !chosen.contains(id)).collect()
                };
                chosen.extend(choose_random(&rest, need - chosen.len(), &mut rng));
            }
            for (i, id) in chosen.iter().enumerate() {
                tx.execute(
                    "INSERT OR IGNORE INTO daily_picks (day, item_id, position) VALUES (?1, ?2, ?3)",
                    params![day, id, (picked.len() + i) as i64],
                )?;
            }
            picked.extend(chosen);
        }
        tx.commit()?;
        picked.truncate(count);
        let items = self.items_by_ids(&picked)?;
        Ok(DailyPickup {
            day: day.to_string(),
            items,
        })
    }

    // ---------------------------------------------------------------- media

    pub fn local_media_paths(&self, urls: &[String]) -> Result<HashMap<String, String>> {
        let mut out = HashMap::new();
        if urls.is_empty() {
            return Ok(out);
        }
        for chunk in urls.chunks(400) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT url, path FROM media_files WHERE url IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(params_from_iter(chunk.iter()), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (u, p) = row?;
                out.insert(u, p);
            }
        }
        Ok(out)
    }

    pub fn record_media_file(&self, url: &str, path: &str, bytes: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO media_files (url, path, bytes, fetched_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(url) DO UPDATE SET path = excluded.path, bytes = excluded.bytes, fetched_at = excluded.fetched_at",
            params![url, path, bytes, now_iso()],
        )?;
        Ok(())
    }

    /// まだローカルに保存していない画像 URL（新しいアイテムから順に）
    pub fn media_urls_to_download(&self, item_ids: Option<&[i64]>, limit: usize) -> Result<Vec<String>> {
        let items = match item_ids {
            Some(ids) => self.items_by_ids(ids)?,
            None => {
                self.list_items(&ItemQuery {
                    limit: Some(500),
                    ..Default::default()
                })?
                .items
            }
        };
        let mut urls = Vec::new();
        for it in &items {
            if let (Some(u), None) = (&it.author_avatar, &it.author_avatar_local) {
                urls.push(u.clone());
            }
            if let (Some(u), None) = (&it.image_url, &it.image_local) {
                urls.push(u.clone());
            }
            for m in &it.media {
                if let (Some(u), None) = (m.image_url(), &m.local_path) {
                    urls.push(u.to_string());
                }
            }
            if let Some(LinkCard {
                image: Some(u),
                image_local: None,
                ..
            }) = &it.link
            {
                urls.push(u.clone());
            }
            if let Some(LinkedPage {
                image_url: Some(u),
                image_local: None,
                ..
            }) = &it.linked
            {
                urls.push(u.clone());
            }
        }
        urls.sort();
        urls.dedup();
        urls.truncate(limit);
        Ok(urls)
    }

    /// タグ付けに使うため、アイテムの本文を省略せずに取る
    /// 紹介先ページの本文もあわせて返す
    pub fn item_for_tagging(&self, id: i64) -> Result<Option<(Item, Option<String>)>> {
        Ok(self.get_item(id)?.map(|d| (d.item, d.linked_text)))
    }

    // ---------------------------------------------------------------- 紹介先ページ

    /// 紹介先ページをまだ保存していないポスト（id と URL）。retry_errors なら前回失敗したものも含める。
    pub fn items_needing_link(
        &self,
        ids: Option<&[i64]>,
        retry_errors: bool,
        limit: i64,
    ) -> Result<Vec<(i64, String)>> {
        let mut sql = String::from(
            "SELECT i.id, i.link_json FROM items i LEFT JOIN linked_pages lp ON lp.item_id = i.id \
             WHERE i.link_json IS NOT NULL",
        );
        sql.push_str(if retry_errors {
            " AND (lp.item_id IS NULL OR lp.error IS NOT NULL)"
        } else {
            " AND lp.item_id IS NULL"
        });
        let mut args: Vec<SqlValue> = Vec::new();
        if let Some(ids) = ids {
            if ids.is_empty() {
                return Ok(vec![]);
            }
            sql.push_str(&format!(" AND i.id IN ({})", vec!["?"; ids.len()].join(",")));
            args.extend(ids.iter().map(|id| SqlValue::Integer(*id)));
        }
        sql.push_str(&format!(" ORDER BY i.saved_at DESC LIMIT {}", limit.max(0)));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, json) = row?;
            if let Ok(card) = serde_json::from_str::<LinkCard>(&json) {
                out.push((id, card.url));
            }
        }
        Ok(out)
    }

    pub fn count_items_needing_link(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM items i LEFT JOIN linked_pages lp ON lp.item_id = i.id \
             WHERE i.link_json IS NOT NULL AND (lp.item_id IS NULL OR lp.error IS NOT NULL)",
            [],
            |r| r.get(0),
        )?)
    }

    /// 紹介先ページを保存する（失敗したときは理由だけ残し、次回まとめて再試行できるようにする）
    pub fn save_linked_page(
        &mut self,
        item_id: i64,
        url: &str,
        page: std::result::Result<&NewItem, &str>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        match page {
            Ok(p) => {
                tx.execute(
                    "INSERT OR REPLACE INTO linked_pages \
                        (item_id, url, title, site_name, author, excerpt, image_url, published_at, content_html, text, fetched_at, error) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL)",
                    params![
                        item_id,
                        url,
                        p.title,
                        p.site_name,
                        p.author_name,
                        p.excerpt,
                        p.image_url,
                        p.published_at,
                        p.content_html,
                        p.text,
                        now_iso()
                    ],
                )?;
                // リンクカードにタイトルがなければ、取得したページのタイトルで補う
                if let Some(json) = tx
                    .query_row("SELECT link_json FROM items WHERE id = ?1", [item_id], |r| {
                        r.get::<_, Option<String>>(0)
                    })
                    .optional()?
                    .flatten()
                {
                    if let Ok(mut card) = serde_json::from_str::<LinkCard>(&json) {
                        let mut changed = false;
                        if card.title.is_none() && p.title.is_some() {
                            card.title = p.title.clone();
                            changed = true;
                        }
                        if card.description.is_none() && p.excerpt.is_some() {
                            card.description = p.excerpt.clone();
                            changed = true;
                        }
                        if card.image.is_none() && p.image_url.is_some() {
                            card.image = p.image_url.clone();
                            changed = true;
                        }
                        if changed {
                            tx.execute(
                                "UPDATE items SET link_json = ?2 WHERE id = ?1",
                                params![item_id, serde_json::to_string(&card)?],
                            )?;
                        }
                    }
                }
            }
            Err(e) => {
                // 以前に保存できていた本文は消さない
                tx.execute(
                    "INSERT INTO linked_pages (item_id, url, fetched_at, error) VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(item_id) DO UPDATE SET fetched_at = excluded.fetched_at, \
                        error = CASE WHEN linked_pages.content_html IS NULL THEN excluded.error ELSE NULL END",
                    params![item_id, url, now_iso(), e],
                )?;
            }
        }
        reindex_tx(&tx, item_id)?;
        tx.commit()?;
        Ok(())
    }
}

// -------------------------------------------------------------------- helpers

fn item_columns(truncate_text: bool) -> String {
    let text = if truncate_text {
        format!("substr(text, 1, {LIST_TEXT_CHARS})")
    } else {
        "text".to_string()
    };
    ITEM_COLUMNS.replace("{TEXT}", &text)
}

fn row_to_item(r: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    let media_json: Option<String> = r.get(13)?;
    let link_json: Option<String> = r.get(14)?;
    let metrics_json: Option<String> = r.get(15)?;
    Ok(Item {
        id: r.get(0)?,
        kind: r.get(1)?,
        url: r.get(2)?,
        external_id: r.get(3)?,
        title: r.get(4)?,
        author_name: r.get(5)?,
        author_handle: r.get(6)?,
        author_avatar: r.get(7)?,
        author_avatar_local: None,
        site_name: r.get(8)?,
        text: r.get(9)?,
        excerpt: r.get(10)?,
        summary: r.get(11)?,
        image_url: r.get(12)?,
        image_local: None,
        media: media_json
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        link: link_json.and_then(|s| serde_json::from_str(&s).ok()),
        metrics: metrics_json.and_then(|s| serde_json::from_str(&s).ok()),
        published_at: r.get(16)?,
        saved_at: r.get(17)?,
        is_liked: r.get::<_, i64>(18)? != 0,
        is_bookmarked: r.get::<_, i64>(19)? != 0,
        is_manual: r.get::<_, i64>(20)? != 0,
        ai_tagged_at: r.get(21)?,
        note: r.get(22)?,
        has_content: r.get::<_, i64>(23).map(|v| v != 0).unwrap_or(false),
        tags: vec![],
        linked: None,
    })
}

fn upsert_item_tx(
    tx: &rusqlite::Transaction<'_>,
    item: &NewItem,
    flags: SourceFlags,
    saved_at: &str,
) -> Result<UpsertResult> {
    let existing: Option<(i64, bool, bool, bool)> = tx
        .query_row(
            "SELECT id, is_liked, is_bookmarked, is_manual FROM items WHERE key = ?1",
            [&item.key],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get::<_, i64>(1)? != 0,
                    r.get::<_, i64>(2)? != 0,
                    r.get::<_, i64>(3)? != 0,
                ))
            },
        )
        .optional()?;

    let media_json = if item.media.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&item.media)?)
    };
    let link_json = item.link.as_ref().map(serde_json::to_string).transpose()?;
    let metrics_json = item.metrics.as_ref().map(|m| m.to_string());
    let now = now_iso();

    tx.execute(
        "INSERT INTO items (key, kind, url, external_id, title, author_name, author_handle, author_avatar, site_name, \
            text, content_html, excerpt, image_url, media_json, link_json, metrics_json, lang, published_at, \
            saved_at, updated_at, is_liked, is_bookmarked, is_manual, raw_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24) \
         ON CONFLICT(key) DO UPDATE SET \
            url = excluded.url, \
            title = COALESCE(excluded.title, items.title), \
            author_name = COALESCE(excluded.author_name, items.author_name), \
            author_handle = COALESCE(excluded.author_handle, items.author_handle), \
            author_avatar = COALESCE(excluded.author_avatar, items.author_avatar), \
            site_name = COALESCE(excluded.site_name, items.site_name), \
            text = CASE WHEN excluded.text <> '' THEN excluded.text ELSE items.text END, \
            content_html = COALESCE(excluded.content_html, items.content_html), \
            excerpt = COALESCE(excluded.excerpt, items.excerpt), \
            image_url = COALESCE(excluded.image_url, items.image_url), \
            media_json = COALESCE(excluded.media_json, items.media_json), \
            link_json = COALESCE(excluded.link_json, items.link_json), \
            metrics_json = COALESCE(excluded.metrics_json, items.metrics_json), \
            lang = COALESCE(excluded.lang, items.lang), \
            published_at = COALESCE(excluded.published_at, items.published_at), \
            updated_at = excluded.updated_at, \
            is_liked = MAX(items.is_liked, excluded.is_liked), \
            is_bookmarked = MAX(items.is_bookmarked, excluded.is_bookmarked), \
            is_manual = MAX(items.is_manual, excluded.is_manual), \
            raw_json = COALESCE(excluded.raw_json, items.raw_json)",
        params![
            item.key,
            item.kind,
            item.url,
            item.external_id,
            item.title,
            item.author_name,
            item.author_handle,
            item.author_avatar,
            item.site_name,
            item.text,
            item.content_html,
            item.excerpt,
            item.image_url,
            media_json,
            link_json,
            metrics_json,
            item.lang,
            item.published_at,
            saved_at,
            now,
            flags.liked as i64,
            flags.bookmarked as i64,
            flags.manual as i64,
            item.raw_json,
        ],
    )?;

    let (id, newly_flagged) = match existing {
        Some((id, liked, bookmarked, manual)) => {
            let newly = (flags.liked && !liked) || (flags.bookmarked && !bookmarked) || (flags.manual && !manual);
            (id, newly)
        }
        None => (tx.last_insert_rowid(), true),
    };
    reindex_tx(tx, id)?;
    Ok(UpsertResult { id, newly_flagged })
}

/// 全文検索用の索引を作り直す
fn reindex_tx(tx: &rusqlite::Transaction<'_>, id: i64) -> Result<()> {
    tx.execute("DELETE FROM items_fts WHERE rowid = ?1", [id])?;
    tx.execute(
        "INSERT INTO items_fts (rowid, title, body, author, note, summary) \
         SELECT i.id, COALESCE(i.title, ''), \
                i.text || ' ' || COALESCE(i.excerpt, '') || ' ' || COALESCE(lp.title, '') || ' ' || COALESCE(lp.text, ''), \
                TRIM(COALESCE(i.author_name, '') || ' @' || COALESCE(i.author_handle, '') || ' ' || COALESCE(i.site_name, '') \
                     || ' ' || COALESCE(lp.site_name, '')), \
                COALESCE(i.note, ''), COALESCE(i.summary, '') \
         FROM items i LEFT JOIN linked_pages lp ON lp.item_id = i.id WHERE i.id = ?1",
        [id],
    )?;
    Ok(())
}

fn ensure_tag(tx: &rusqlite::Transaction<'_>, name: &str) -> Result<i64> {
    tx.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [name])?;
    Ok(tx.query_row("SELECT id FROM tags WHERE name = ?1", [name], |r| r.get(0))?)
}

fn delete_orphan_tags(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    tx.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM item_tags)",
        [],
    )?;
    Ok(())
}

/// タグ名を整える（前後の空白・先頭の # を取り、連続空白を 1 つに）
pub fn normalize_tag(raw: &str) -> Option<String> {
    let s = raw.trim().trim_start_matches(['#', '＃']).trim();
    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s: String = s.chars().take(40).collect();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn normalize_tags(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in raw.iter().filter_map(|t| normalize_tag(t)) {
        if !out.iter().any(|o| o.to_lowercase() == t.to_lowercase()) {
            out.push(t);
        }
    }
    out
}

/// キーワードを空白（全角含む）で区切る
pub fn split_keywords(keyword: &str) -> Vec<String> {
    keyword
        .split(|c: char| c.is_whitespace() || c == '\u{3000}')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn build_where(q: &ItemQuery) -> (String, Vec<SqlValue>) {
    let mut conds: Vec<String> = vec!["1 = 1".into()];
    let mut args: Vec<SqlValue> = Vec::new();

    match q.filter.as_deref() {
        Some("liked") => conds.push("i.is_liked = 1".into()),
        Some("bookmarked") => conds.push("i.is_bookmarked = 1".into()),
        Some("tweet") => conds.push("i.kind = 'tweet'".into()),
        Some("article") => conds.push("i.kind = 'article'".into()),
        Some("manual") => conds.push("i.is_manual = 1".into()),
        Some("untagged") => conds.push("NOT EXISTS (SELECT 1 FROM item_tags it WHERE it.item_id = i.id)".into()),
        _ => {}
    }

    if let Some(kw) = q.keyword.as_deref() {
        for term in split_keywords(kw) {
            let (negate, term) = match term.strip_prefix('-') {
                Some(t) if !t.is_empty() => (true, t.to_string()),
                _ => (false, term),
            };
            let cond = if term.chars().count() >= 3 {
                // trigram 索引で検索（フレーズとして扱う）
                args.push(SqlValue::Text(format!("\"{}\"", term.replace('"', "\"\""))));
                "i.id IN (SELECT rowid FROM items_fts WHERE items_fts MATCH ?)".to_string()
            } else {
                // 2 文字以下は trigram で引けないので LIKE で探す
                let pat = format!("%{}%", escape_like(&term));
                for _ in 0..9 {
                    args.push(SqlValue::Text(pat.clone()));
                }
                "(i.title LIKE ? ESCAPE '\\' OR i.text LIKE ? ESCAPE '\\' OR i.author_name LIKE ? ESCAPE '\\' \
                  OR i.author_handle LIKE ? ESCAPE '\\' OR i.site_name LIKE ? ESCAPE '\\' OR i.note LIKE ? ESCAPE '\\' \
                  OR i.summary LIKE ? ESCAPE '\\' \
                  OR i.id IN (SELECT item_id FROM linked_pages WHERE title LIKE ? ESCAPE '\\' OR text LIKE ? ESCAPE '\\'))"
                    .to_string()
            };
            conds.push(if negate { format!("NOT {cond}") } else { cond });
        }
    }

    let tags = normalize_tags(&q.tags);
    if !tags.is_empty() {
        let placeholders = vec!["?"; tags.len()].join(",");
        let n = tags.len();
        for t in &tags {
            args.push(SqlValue::Text(t.clone()));
        }
        if q.tag_mode.as_deref() == Some("or") {
            conds.push(format!(
                "i.id IN (SELECT it.item_id FROM item_tags it JOIN tags t ON t.id = it.tag_id WHERE t.name IN ({placeholders}))"
            ));
        } else {
            conds.push(format!(
                "i.id IN (SELECT it.item_id FROM item_tags it JOIN tags t ON t.id = it.tag_id WHERE t.name IN ({placeholders}) \
                 GROUP BY it.item_id HAVING COUNT(DISTINCT t.id) = {n})"
            ));
        }
    }

    (conds.join(" AND "), args)
}

fn choose_random<R: Rng + ?Sized>(pool: &[i64], n: usize, rng: &mut R) -> Vec<i64> {
    let mut v = pool.to_vec();
    v.shuffle(rng);
    v.truncate(n);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tweet(id: &str, text: &str) -> NewItem {
        NewItem {
            key: format!("tweet:{id}"),
            kind: KIND_TWEET.into(),
            url: format!("https://x.com/someone/status/{id}"),
            external_id: Some(id.into()),
            author_name: Some("テスト太郎".into()),
            author_handle: Some("taro".into()),
            text: text.into(),
            ..Default::default()
        }
    }

    fn article(url: &str, title: &str, text: &str) -> NewItem {
        NewItem {
            key: format!("url:{url}"),
            kind: KIND_ARTICLE.into(),
            url: url.into(),
            title: Some(title.into()),
            site_name: Some("Example".into()),
            text: text.into(),
            content_html: Some("<p>本文</p>".into()),
            ..Default::default()
        }
    }

    fn liked() -> SourceFlags {
        SourceFlags {
            liked: true,
            ..Default::default()
        }
    }

    #[test]
    fn upsert_merges_flags_and_reports_newly_flagged() {
        let mut db = Db::open_in_memory().unwrap();
        let t = tweet("1", "hello");
        let r1 = db.upsert_item(&t, liked(), &now_iso()).unwrap();
        assert!(r1.newly_flagged);
        let r2 = db.upsert_item(&t, liked(), &now_iso()).unwrap();
        assert!(!r2.newly_flagged, "同じフラグで再保存しても新規扱いにしない");
        assert_eq!(r1.id, r2.id);
        let r3 = db
            .upsert_item(
                &t,
                SourceFlags {
                    bookmarked: true,
                    ..Default::default()
                },
                &now_iso(),
            )
            .unwrap();
        assert!(r3.newly_flagged);
        let item = db.get_item(r1.id).unwrap().unwrap().item;
        assert!(item.is_liked && item.is_bookmarked);
    }

    #[test]
    fn keyword_search_handles_japanese_short_and_long_terms() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_item(&tweet("1", "Rust で設計パターンを学ぶ"), liked(), &now_iso())
            .unwrap();
        db.upsert_item(
            &article("https://example.com/a", "料理のコツ", "美味しいカレーの作り方"),
            SourceFlags::default(),
            &now_iso(),
        )
        .unwrap();

        let search = |db: &Db, kw: &str| {
            db.list_items(&ItemQuery {
                keyword: Some(kw.into()),
                ..Default::default()
            })
            .unwrap()
            .total
        };
        assert_eq!(search(&db, "設計"), 1, "2 文字は LIKE で探す");
        assert_eq!(search(&db, "設計パターン"), 1, "3 文字以上は FTS");
        assert_eq!(search(&db, "カレーの作り方"), 1);
        assert_eq!(search(&db, "rust"), 1, "大文字小文字を区別しない");
        assert_eq!(search(&db, "Rust　設計"), 1, "全角スペース区切りの AND");
        assert_eq!(search(&db, "Rust カレー"), 0);
        assert_eq!(search(&db, "-カレー"), 1, "除外検索");
        assert_eq!(search(&db, "太郎"), 1, "投稿者名でも探せる");
        assert_eq!(search(&db, "100%"), 0, "LIKE の特殊文字をエスケープ");
    }

    #[test]
    fn tag_filter_and_or_and_rename_merge() {
        let mut db = Db::open_in_memory().unwrap();
        let a = db.upsert_item(&tweet("1", "a"), liked(), &now_iso()).unwrap().id;
        let b = db.upsert_item(&tweet("2", "b"), liked(), &now_iso()).unwrap().id;
        db.set_item_tags(a, &["Rust".into(), "#設計".into(), "rust".into()], "manual")
            .unwrap();
        db.set_item_tags(b, &["Rust".into(), "料理".into()], "manual").unwrap();
        assert_eq!(db.item_tags(a).unwrap(), vec!["Rust", "設計"]);

        let q = |tags: &[&str], mode: &str| ItemQuery {
            tags: tags.iter().map(|s| s.to_string()).collect(),
            tag_mode: Some(mode.into()),
            ..Default::default()
        };
        assert_eq!(db.list_items(&q(&["rust"], "and")).unwrap().total, 2);
        assert_eq!(db.list_items(&q(&["Rust", "設計"], "and")).unwrap().total, 1);
        assert_eq!(db.list_items(&q(&["設計", "料理"], "or")).unwrap().total, 2);

        db.rename_tag("設計", "Rust").unwrap();
        let tags = db.list_tags().unwrap();
        assert_eq!(
            tags[0],
            TagCount {
                name: "Rust".into(),
                count: 2
            }
        );
        assert!(!tags.iter().any(|t| t.name == "設計"));

        db.set_item_tags(b, &[], "manual").unwrap();
        assert!(
            !db.list_tags().unwrap().iter().any(|t| t.name == "料理"),
            "使われなくなったタグは消える"
        );
        assert_eq!(
            db.list_items(&ItemQuery {
                filter: Some("untagged".into()),
                ..Default::default()
            })
            .unwrap()
            .total,
            1
        );
    }

    #[test]
    fn daily_pickup_is_stable_within_a_day_and_avoids_recent() {
        let mut db = Db::open_in_memory().unwrap();
        for i in 0..20 {
            db.upsert_item(&tweet(&i.to_string(), "x"), liked(), &now_iso())
                .unwrap();
        }
        let p1 = db.daily_pickup("2026-09-01", 5, false).unwrap();
        let p2 = db.daily_pickup("2026-09-01", 5, false).unwrap();
        let ids1: Vec<i64> = p1.items.iter().map(|i| i.id).collect();
        let ids2: Vec<i64> = p2.items.iter().map(|i| i.id).collect();
        assert_eq!(ids1.len(), 5);
        assert_eq!(ids1, ids2, "同じ日は同じ結果");

        let p3 = db.daily_pickup("2026-09-02", 5, false).unwrap();
        assert!(p3.items.iter().all(|i| !ids1.contains(&i.id)), "直近に出たものは避ける");

        let p4 = db.daily_pickup("2026-09-02", 8, true).unwrap();
        assert_eq!(p4.items.len(), 8);
    }

    #[test]
    fn stats_and_delete() {
        let mut db = Db::open_in_memory().unwrap();
        let a = db.upsert_item(&tweet("1", "a"), liked(), &now_iso()).unwrap().id;
        db.upsert_item(
            &article("https://e.com/x", "t", "body"),
            SourceFlags {
                manual: true,
                ..Default::default()
            },
            &now_iso(),
        )
        .unwrap();
        db.set_item_tags(a, &["x".into()], "manual").unwrap();
        let s = db.stats().unwrap();
        assert_eq!(
            (s.total, s.liked, s.tweets, s.articles, s.manual, s.untagged, s.tags),
            (2, 1, 1, 1, 1, 1, 1)
        );
        db.delete_item(a).unwrap();
        let s = db.stats().unwrap();
        assert_eq!((s.total, s.tags), (1, 0));
        assert_eq!(
            db.list_items(&ItemQuery {
                keyword: Some("テスト太郎".into()),
                ..Default::default()
            })
            .unwrap()
            .total,
            0,
            "削除したアイテムは全文検索の索引からも消える"
        );
    }

    #[test]
    fn list_text_is_truncated_but_detail_is_full() {
        let mut db = Db::open_in_memory().unwrap();
        let long = "あ".repeat(2000);
        let id = db
            .upsert_item(
                &article("https://e.com/long", "t", &long),
                SourceFlags::default(),
                &now_iso(),
            )
            .unwrap()
            .id;
        let listed = &db.list_items(&ItemQuery::default()).unwrap().items[0];
        assert_eq!(listed.text.chars().count(), LIST_TEXT_CHARS as usize);
        assert!(listed.has_content);
        assert_eq!(db.get_item(id).unwrap().unwrap().item.text.chars().count(), 2000);
    }

    #[test]
    fn linked_page_is_saved_searchable_and_retried_on_error() {
        let mut db = Db::open_in_memory().unwrap();
        let mut t = tweet("1", "これ良かった");
        t.link = Some(LinkCard {
            url: "https://blog.example.com/a".into(),
            ..Default::default()
        });
        let id = db.upsert_item(&t, liked(), &now_iso()).unwrap().id;
        let plain = db
            .upsert_item(&tweet("2", "リンクなし"), liked(), &now_iso())
            .unwrap()
            .id;

        assert_eq!(
            db.items_needing_link(None, false, 10).unwrap(),
            vec![(id, "https://blog.example.com/a".to_string())],
            "リンクのあるポストだけが対象"
        );
        assert!(db.items_needing_link(Some(&[plain]), false, 10).unwrap().is_empty());

        // 失敗したときは理由だけ残し、retry_errors で再取得の対象になる
        db.save_linked_page(id, "https://blog.example.com/a", Err("404 Not Found"))
            .unwrap();
        assert!(db.items_needing_link(None, false, 10).unwrap().is_empty());
        assert_eq!(db.items_needing_link(None, true, 10).unwrap().len(), 1);
        assert_eq!(db.count_items_needing_link().unwrap(), 1);
        let linked = db.get_item(id).unwrap().unwrap().item.linked.unwrap();
        assert_eq!(linked.error.as_deref(), Some("404 Not Found"));
        assert!(!linked.has_content);

        let page = NewItem {
            title: Some("全文検索の作り方".into()),
            site_name: Some("Example Blog".into()),
            excerpt: Some("概要".into()),
            image_url: Some("https://blog.example.com/cover.png".into()),
            content_html: Some("<p>形態素解析なしで検索する</p>".into()),
            text: "形態素解析なしで検索する方法".into(),
            ..Default::default()
        };
        db.save_linked_page(id, "https://blog.example.com/a?final", Ok(&page))
            .unwrap();
        assert_eq!(db.count_items_needing_link().unwrap(), 0);

        let detail = db.get_item(id).unwrap().unwrap();
        let linked = detail.item.linked.clone().unwrap();
        assert!(linked.has_content && linked.error.is_none());
        assert_eq!(linked.url, "https://blog.example.com/a?final");
        assert_eq!(detail.linked_html.as_deref(), Some("<p>形態素解析なしで検索する</p>"));
        let card = detail.item.link.clone().unwrap();
        assert_eq!(
            card.title.as_deref(),
            Some("全文検索の作り方"),
            "カードのタイトルを補う"
        );

        let search = |db: &Db, kw: &str| {
            db.list_items(&ItemQuery {
                keyword: Some(kw.into()),
                ..Default::default()
            })
            .unwrap()
            .total
        };
        assert_eq!(search(&db, "形態素解析"), 1, "紹介先の本文でポストが見つかる");
        assert_eq!(search(&db, "検索"), 1, "2 文字でも見つかる");
        assert_eq!(search(&db, "Example Blog"), 1);

        // あとから失敗しても、保存済みの本文は消さない
        db.save_linked_page(id, "https://blog.example.com/a", Err("timeout"))
            .unwrap();
        let linked = db.get_item(id).unwrap().unwrap().item.linked.unwrap();
        assert!(linked.has_content && linked.error.is_none());

        // 紹介先のカバー画像も保存対象になる
        let todo = db.media_urls_to_download(Some(&[id]), 10).unwrap();
        assert!(todo.contains(&"https://blog.example.com/cover.png".to_string()));

        db.delete_item(id).unwrap();
        assert_eq!(search(&db, "形態素解析"), 0);
    }

    #[test]
    fn migrates_v1_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
        }
        let db = Db::open(&path).unwrap();
        let v: i64 = db.conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        assert_eq!(db.count_items_needing_link().unwrap(), 0, "linked_pages が作られている");
    }

    #[test]
    fn local_media_paths_are_attached() {
        let mut db = Db::open_in_memory().unwrap();
        let mut t = tweet("1", "pic");
        t.author_avatar = Some("https://pbs.twimg.com/a.jpg".into());
        t.media = vec![Media {
            kind: "photo".into(),
            url: Some("https://pbs.twimg.com/m.jpg".into()),
            ..Default::default()
        }];
        let id = db.upsert_item(&t, liked(), &now_iso()).unwrap().id;
        let todo = db.media_urls_to_download(Some(&[id]), 10).unwrap();
        assert_eq!(todo.len(), 2);
        db.record_media_file("https://pbs.twimg.com/m.jpg", "/tmp/m.jpg", 10)
            .unwrap();
        let item = db.get_item(id).unwrap().unwrap().item;
        assert_eq!(item.media[0].local_path.as_deref(), Some("/tmp/m.jpg"));
        assert_eq!(
            db.media_urls_to_download(Some(&[id]), 10).unwrap(),
            vec!["https://pbs.twimg.com/a.jpg".to_string()]
        );
    }
}
