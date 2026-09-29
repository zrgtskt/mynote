mod article;
mod cli;
mod commands;
mod core;
mod db;
mod models;
mod tagger;
mod x_api;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::core::{default_data_dir, Core};

/// アプリを開いている間、1 時間ごとに「前回の同期から 20 時間以上たったか」を確かめて同期する
async fn auto_sync_loop(core: Arc<Core>, app: tauri::AppHandle) {
    tokio::time::sleep(Duration::from_secs(8)).await;
    loop {
        if core.should_auto_sync() {
            if let Ok((_, ids)) = core.sync_x(false).await {
                commands::notify_changed(&app);
                core.postprocess(&ids).await;
                commands::notify_changed(&app);
            }
        }
        tokio::time::sleep(Duration::from_secs(60 * 60)).await;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| cli::is_cli_command(a)) {
        std::process::exit(cli::run(&args));
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = default_data_dir()?;
            let handle = app.handle().clone();
            let core = Arc::new(Core::new(
                data_dir,
                Box::new(move |p| {
                    let _ = handle.emit("progress", p);
                }),
            )?);
            // 保存した画像を画面から読めるようにする
            app.asset_protocol_scope().allow_directory(&core.media_dir, true)?;
            app.manage(core.clone());
            tauri::async_runtime::spawn(auto_sync_loop(core, app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::x_connect,
            commands::x_disconnect,
            commands::sync_now,
            commands::list_items,
            commands::get_item,
            commands::get_stats,
            commands::list_tags,
            commands::set_item_tags,
            commands::rename_tag,
            commands::delete_tag,
            commands::set_note,
            commands::delete_item,
            commands::add_url,
            commands::tag_pending,
            commands::count_pending_tags,
            commands::retag_item,
            commands::refetch_linked,
            commands::fetch_linked_pending,
            commands::count_pending_links,
            commands::daily_pickup,
            commands::download_media,
            commands::open_data_dir,
            commands::open_external,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
