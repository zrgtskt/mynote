# mynote

X（旧 Twitter）の **いいね・ブックマーク** と、気になった **Web 記事** をローカルに保存し、**タグで整理**して読み返すためのデスクトップアプリです。
Tauri 2（Rust + React）で作っていて、データはすべて手元のパソコンに保存されます。

![アイコン](assets/icon.svg)

## できること

| 機能 | 内容 |
|---|---|
| X の自動取得 | 公式 API（従量課金）で、自分のいいね・ブックマークを取得。2 回目以降は新着だけを取りに行き、アプリを開いている間は 1 日 1 回自動で同期 |
| Web 記事の保存 | URL を貼るだけで本文を取り出して保存（Readability 方式）。追跡用パラメータ（`utm_*` など）は自動で除去 |
| 紹介先ページの保存 | ポストにリンクがあれば、リンク先のページも本文・カバー画像ごと保存し、ポストの詳細画面で読める。紹介先の本文もキーワード検索と AI タグ付けに使う |
| 画像の保存 | ツイートの画像・アイコン・リンクカード・記事のカバー画像もローカルに保存。元のポストが消えても残る |
| 自動タグ付け | Claude API がタグと一行要約を付ける。既存のタグを優先して使うので表記ゆれが増えにくい |
| 今日のピックアップ | 毎日ランダムに数件を表示（その日のうちは固定、直近 30 日に出たものは避ける） |
| タグ検索 | 複数タグの「すべて含む（AND）」「いずれか（OR）」で絞り込み |
| タグ一覧 | 件数付きの一覧とタグクラウド。名前の変更、既存タグへの統合、削除 |
| キーワード検索 | 本文・タイトル・投稿者・メモ・要約を全文検索（日本語対応、スペースで AND、`-語` で除外） |
| メモ | アイテムごとにメモを残せる（検索の対象にもなる） |

ダーク / ライトテーマは OS の設定に合わせて切り替わります。

## 必要なもの

- [Node.js](https://nodejs.org/) 20 以上
- [Rust](https://www.rust-lang.org/tools/install)（stable）
- OS ごとの Tauri の前提ライブラリ: <https://v2.tauri.app/start/prerequisites/>
  - macOS: Xcode Command Line Tools（`xcode-select --install`）
  - Windows: WebView2（Windows 10/11 は標準で入っています）と Microsoft C++ Build Tools
  - Linux (Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`

## 起動

```bash
npm install
npm run app        # 開発モードで起動（tauri dev）
```

配布用のアプリ（.app / .dmg / .msi / .deb など）を作るには:

```bash
npm run bundle     # tauri build → src-tauri/target/release/bundle/ に出力
```

## X と連携する（従量課金 API）

1. [X の開発者ポータル](https://developer.x.com/en/portal/dashboard) でアプリ（Project / App）を作り、クレジットをチャージします。
2. アプリの **User authentication settings** で OAuth 2.0 を有効にし、次のように設定します。
   - App permissions: **Read**
   - Type of App: **Native App**（公開クライアント。Client Secret は不要）
   - Callback URI / Redirect URL: `http://127.0.0.1:8723/callback`
   - Website URL: 任意（例: `https://example.com`）
3. 表示された **OAuth 2.0 Client ID** を、アプリの「設定 → X 連携」に貼り付けて「連携する」を押します。
4. ブラウザで X の許可画面が開くので許可します。以降は「同期」ボタン、または自動同期でいいね・ブックマークが取り込まれます。

> Web App 型（機密クライアント）で作った場合は、Client Secret も設定画面に入力してください。

### 料金の目安

- 自分のいいね・ブックマークの取得は「Owned Reads」として **1 件あたり約 $0.001**（1,000 件で約 $1）です（2026 年 4 月時点の X の料金）。
- 初回は「1 回の上限」（初期値 1,000 件）まで取得します。
- 2 回目以降は、まず新しいほうから 20 件だけ確認し、前回までに取り込んだところに届いたらそこで止めます。毎日の同期は数円程度です。
- X のブックマーク API は、新しいほうから 800 件までしか返さないことがあります。
- 料金は変わることがあるので、最新の情報は X の開発者ポータルで確認してください。

## Claude で自動タグ付け

1. [Anthropic Console](https://console.anthropic.com/) で API キーを発行します。
2. アプリの「設定 → Claude で自動タグ付け」にキーを保存します（環境変数 `ANTHROPIC_API_KEY` があればそれも使えます）。
3. 「保存時に自動でタグ付け」がオンなら、同期や URL の追加のあとに自動で実行されます。既にあるものは「未処理の N 件をタグ付け」でまとめて処理できます。

- モデルの初期値は `claude-opus-5-5` です。件数が多くて費用を抑えたいときは `claude-haiku-4-5` に変更できます。
- 1 件ごとに、既存のタグ一覧（使用数の多い順に最大 300 個）と、本文を送ります。記事は本文の**先頭 6,000 文字**、ポストの紹介先ページは**先頭 3,000 文字**だけを送ります。
- 料金の目安は 1 件あたり数円です（モデルと本文の長さによって変わります）。
- 手動で付けたタグは残ります。詳細画面の「AI タグ」で、AI が付けたタグだけを付け直せます。

## コマンドラインで使う（cron などで毎日同期）

アプリの実行ファイルは、引数を付けると画面を開かずに動きます。

```bash
mynote sync            # X の新着いいね・ブックマークを同期 → 画像保存 → 自動タグ付け
mynote sync --full     # 上限まで全件を取り直す
mynote add <URL>...    # 記事や X のポストを追加
mynote links [--retry] # ポストの紹介先ページをまとめて保存（--retry で前回失敗したものも再取得）
mynote tag             # 未処理のものを Claude でタグ付け
mynote help
```

アプリを閉じていても毎日同期したい場合は、cron（macOS / Linux）やタスク スケジューラ（Windows）に `mynote sync` を登録します。

```cron
# 毎朝 7:05 に同期（パスは各自の環境に合わせてください）
5 7 * * * /Applications/mynote.app/Contents/MacOS/mynote sync >> ~/mynote-sync.log 2>&1
```

## データの場所

| OS | 場所 |
|---|---|
| macOS | `~/Library/Application Support/dev.mynote.desktop/` |
| Windows | `%APPDATA%\dev.mynote.desktop\` |
| Linux | `~/.local/share/dev.mynote.desktop/` |

- `mynote.db`: SQLite データベース（アイテム、タグ、設定、X のトークン、API キー）
- `media/`: 保存した画像

紹介先ページは、同期や追加のあとに自動で取得します（設定の「紹介先のページも保存」で切り替え）。サイトが取得を拒否した場合や HTML 以外（PDF・動画など）の場合は保存できず、理由を記録します。詳細画面の「もう一度試す」や設定画面のボタンで取り直せます。記事本文の中にある画像は、元のサイトから読み込みます（ローカルに保存するのはカバー画像だけです）。

環境変数 `MYNOTE_DATA_DIR` で場所を変えられます。バックアップはこのフォルダをコピーするだけです。

**注意:** X のトークンと Anthropic の API キーは、このフォルダの `mynote.db` に暗号化せずに保存されます。フォルダを他人と共有しないでください。保存したデータはリポジトリの外に置かれるので、Git にコミットされることはありません。

## 開発

```bash
npm run dev          # ブラウザだけで画面を確認（仮データで動く。Rust は不要）
npm run typecheck    # TypeScript の型チェック
npm run build        # フロントエンドのビルド
cd src-tauri && cargo test   # Rust のテスト
```

`npm run dev` でブラウザから開くと、`src/mock.ts` の仮データで動きます（開発時だけ。配布用のビルドには含まれません）。

### 構成

```
src/                 画面（React + TypeScript）
  views/             ピックアップ・一覧・タグ一覧・設定
  components/        カード、詳細パネル、タグ入力など
  api.ts             Rust のコマンドを呼ぶ窓口
src-tauri/src/       バックエンド（Rust）
  core.rs            同期・追加・タグ付け・画像保存のまとめ役（画面と CLI で共通）
  db.rs              SQLite（FTS5 trigram で日本語全文検索、ピックアップ）
  x_api.rs           X API v2（OAuth 2.0 PKCE、いいね・ブックマーク取得、応答の解析）
  article.rs         記事の取得と本文抽出（dom_smoothie）、HTML のサニタイズ（ammonia）
  tagger.rs          Claude API でのタグ付け（structured outputs）
  commands.rs        画面から呼ぶ Tauri コマンド
  cli.rs             `mynote sync` などのコマンドライン
```
