//! 画面を開かずに使うコマンド（cron などから毎日の同期に使う）
//!
//!   mynote sync [--full]   X のいいね・ブックマークを同期して、画像保存とタグ付けまで行う
//!   mynote add <URL>...    記事や X のポストを追加する
//!   mynote links           ポストの紹介先ページをまとめて保存する
//!   mynote tag             まだ AI タグ付けしていないものをタグ付けする

use crate::core::{default_data_dir, Core};
use crate::models::Progress;

const HELP: &str = "mynote — X のいいね・ブックマークと Web 記事をローカルに保存してタグで整理

使い方:
  mynote                 アプリを開く
  mynote sync [--full]   X のいいね・ブックマークを同期（--full で全件を取り直す）
  mynote add <URL>...    記事や X のポストを追加
  mynote links [--retry] ポストの紹介先ページをまとめて保存（--retry で失敗分も再取得）
  mynote tag             未タグ付けのものを Claude でタグ付け
  mynote help            このヘルプ

環境変数:
  MYNOTE_DATA_DIR        データの保存先を変える
  ANTHROPIC_API_KEY      設定画面で API キーを入れていないときに使う";

pub fn is_cli_command(arg: &str) -> bool {
    matches!(arg, "sync" | "add" | "links" | "tag" | "help" | "--help" | "-h")
}

pub fn run(args: &[String]) -> i32 {
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    if matches!(cmd, "help" | "--help" | "-h") {
        println!("{HELP}");
        return 0;
    }
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("起動できません: {e}");
            return 1;
        }
    };
    rt.block_on(async {
        let dir = match default_data_dir() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{e:#}");
                return 1;
            }
        };
        let core = match Core::new(
            dir,
            Box::new(|p: Progress| {
                if p.done || p.task != "media" {
                    eprintln!("[{}] {}", p.task, p.message);
                }
            }),
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e:#}");
                return 1;
            }
        };
        match cmd {
            "sync" => {
                let full = args.iter().any(|a| a == "--full");
                match core.sync_x(full).await {
                    Ok((report, ids)) => {
                        let post = core.postprocess(&ids).await;
                        println!("{}", report.summary());
                        if post.links > 0 {
                            println!("紹介先のページ {} 件を保存", post.links);
                        }
                        if post.media > 0 {
                            println!("画像 {} 件を保存", post.media);
                        }
                        if let Some(t) = post.tagged {
                            println!("タグ付け {} 件（失敗 {} 件）", t.tagged, t.failed);
                        }
                        for e in &report.errors {
                            eprintln!("エラー: {e}");
                        }
                        if report.errors.is_empty() {
                            0
                        } else {
                            2
                        }
                    }
                    Err(e) => {
                        eprintln!("同期できません: {e:#}");
                        1
                    }
                }
            }
            "add" => {
                let urls: Vec<&String> = args.iter().skip(2).collect();
                if urls.is_empty() {
                    eprintln!("URL を指定してください: mynote add <URL>");
                    return 1;
                }
                let mut code = 0;
                let mut ids = Vec::new();
                for u in urls {
                    match core.add_url(u).await {
                        Ok(item) => {
                            println!("追加: {}", item.title.as_deref().unwrap_or(&item.url));
                            ids.push(item.id);
                        }
                        Err(e) => {
                            eprintln!("失敗: {u}: {e:#}");
                            code = 1;
                        }
                    }
                }
                core.postprocess(&ids).await;
                code
            }
            "links" => {
                let retry = args.iter().any(|a| a == "--retry");
                match core.fetch_linked_pages(None, retry).await {
                    Ok(n) => {
                        println!("紹介先のページ {n} 件を保存");
                        let _ = core.download_media(None).await;
                        0
                    }
                    Err(e) => {
                        eprintln!("{e:#}");
                        1
                    }
                }
            }
            "tag" => match core.tag_pending(10_000).await {
                Ok(r) => {
                    println!("タグ付け {} 件（失敗 {} 件）", r.tagged, r.failed);
                    for e in &r.errors {
                        eprintln!("エラー: {e}");
                    }
                    0
                }
                Err(e) => {
                    eprintln!("{e:#}");
                    1
                }
            },
            other => {
                eprintln!("不明なコマンド: {other}\n\n{HELP}");
                1
            }
        }
    })
}
