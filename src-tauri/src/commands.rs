//! 画面（フロントエンド）から呼ばれる Tauri コマンド

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::core::Core;
use crate::models::*;

type Res<T> = Result<T, String>;
type CoreState<'a> = State<'a, Arc<Core>>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// 一覧の再読み込みを画面に知らせる
pub fn notify_changed(app: &AppHandle) {
    let _ = app.emit("items-changed", ());
}

#[tauri::command]
pub async fn get_settings(core: CoreState<'_>) -> Res<PublicSettings> {
    Ok(core.settings())
}

#[tauri::command]
pub async fn save_settings(core: CoreState<'_>, patch: SettingsPatch) -> Res<PublicSettings> {
    core.save_settings(patch).map_err(err)
}

#[tauri::command]
pub async fn x_connect(app: AppHandle, core: CoreState<'_>) -> Res<PublicSettings> {
    let opener = app.clone();
    core.x_connect(move |url| {
        opener
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| anyhow::anyhow!("ブラウザを開けません: {e}"))
    })
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn x_disconnect(core: CoreState<'_>) -> Res<PublicSettings> {
    core.x_disconnect().map_err(err)
}

/// X から同期する。画像保存と自動タグ付けは裏で続ける。
#[tauri::command]
pub async fn sync_now(app: AppHandle, core: CoreState<'_>, full: Option<bool>) -> Res<SyncReport> {
    let (report, new_ids) = core.sync_x(full.unwrap_or(false)).await.map_err(err)?;
    notify_changed(&app);
    let core = core.inner().clone();
    tauri::async_runtime::spawn(async move {
        core.postprocess(&new_ids).await;
        notify_changed(&app);
    });
    Ok(report)
}

#[tauri::command]
pub async fn list_items(core: CoreState<'_>, query: ItemQuery) -> Res<ItemPage> {
    core.db().list_items(&query).map_err(err)
}

#[tauri::command]
pub async fn get_item(core: CoreState<'_>, id: i64) -> Res<Option<ItemDetail>> {
    core.db().get_item(id).map_err(err)
}

#[tauri::command]
pub async fn get_stats(core: CoreState<'_>) -> Res<Stats> {
    core.db().stats().map_err(err)
}

#[tauri::command]
pub async fn list_tags(core: CoreState<'_>) -> Res<Vec<TagCount>> {
    core.db().list_tags().map_err(err)
}

#[tauri::command]
pub async fn set_item_tags(core: CoreState<'_>, id: i64, tags: Vec<String>) -> Res<Vec<String>> {
    core.db().set_item_tags(id, &tags, "manual").map_err(err)
}

#[tauri::command]
pub async fn rename_tag(core: CoreState<'_>, from: String, to: String) -> Res<()> {
    core.db().rename_tag(&from, &to).map_err(err)
}

#[tauri::command]
pub async fn delete_tag(core: CoreState<'_>, name: String) -> Res<()> {
    core.db().delete_tag(&name).map_err(err)
}

#[tauri::command]
pub async fn set_note(core: CoreState<'_>, id: i64, note: String) -> Res<()> {
    core.db().set_note(id, &note).map_err(err)
}

#[tauri::command]
pub async fn delete_item(core: CoreState<'_>, id: i64) -> Res<()> {
    core.db().delete_item(id).map_err(err)
}

/// URL（記事または X のポスト）を追加する。画像保存とタグ付けは裏で行う。
#[tauri::command]
pub async fn add_url(app: AppHandle, core: CoreState<'_>, url: String) -> Res<Item> {
    let item = core.add_url(&url).await.map_err(err)?;
    notify_changed(&app);
    let core = core.inner().clone();
    let id = item.id;
    tauri::async_runtime::spawn(async move {
        core.postprocess(&[id]).await;
        notify_changed(&app);
    });
    Ok(item)
}

#[tauri::command]
pub async fn tag_pending(app: AppHandle, core: CoreState<'_>, limit: Option<i64>) -> Res<TagReport> {
    let r = core.tag_pending(limit.unwrap_or(500)).await.map_err(err);
    notify_changed(&app);
    r
}

#[tauri::command]
pub async fn count_pending_tags(core: CoreState<'_>) -> Res<i64> {
    core.db().count_untagged_by_ai().map_err(err)
}

#[tauri::command]
pub async fn retag_item(app: AppHandle, core: CoreState<'_>, id: i64) -> Res<ItemDetail> {
    let r = core.retag_item(id).await.map_err(err);
    notify_changed(&app);
    r
}

/// 1 件の紹介先ページを取り直す
#[tauri::command]
pub async fn refetch_linked(app: AppHandle, core: CoreState<'_>, id: i64) -> Res<ItemDetail> {
    let r = core.refetch_linked_page(id).await.map_err(err);
    notify_changed(&app);
    r
}

/// まだ保存していない（または前回失敗した）紹介先ページをまとめて保存する
#[tauri::command]
pub async fn fetch_linked_pending(app: AppHandle, core: CoreState<'_>) -> Res<i64> {
    let n = core.fetch_linked_pages(None, true).await.map_err(err)?;
    let _ = core.download_media(None).await;
    notify_changed(&app);
    Ok(n)
}

#[tauri::command]
pub async fn count_pending_links(core: CoreState<'_>) -> Res<i64> {
    core.db().count_items_needing_link().map_err(err)
}

#[tauri::command]
pub async fn daily_pickup(core: CoreState<'_>, reshuffle: Option<bool>) -> Res<DailyPickup> {
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let count = core.pickup_count();
    core.db()
        .daily_pickup(&day, count, reshuffle.unwrap_or(false))
        .map_err(err)
}

#[tauri::command]
pub async fn download_media(app: AppHandle, core: CoreState<'_>) -> Res<i64> {
    let n = core.download_media(None).await.map_err(err)?;
    notify_changed(&app);
    Ok(n)
}

#[tauri::command]
pub async fn open_data_dir(app: AppHandle, core: CoreState<'_>) -> Res<()> {
    app.opener()
        .open_path(core.data_dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> Res<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("http / https のリンクだけ開けます".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}
