import { useEffect, useState } from "react";
import { AtSign, Bot, Copy, Download, FolderOpen, Info, KeyRound, LoaderCircle, LogOut, RefreshCw, Sparkles, SlidersHorizontal } from "lucide-react";
import { api, errorMessage, mediaSrc } from "../api";
import { Switch, openExternal } from "../components/common";
import { longDate } from "../lib/format";
import type { Settings, SettingsPatch } from "../types";

interface Props {
  settings: Settings;
  onSettings: (s: Settings) => void;
  onSync: (full: boolean) => void;
  syncing: boolean;
  onChanged: () => void;
  toast: (kind: "success" | "error" | "info", message: string) => void;
}

export function SettingsView({ settings, onSettings, onSync, syncing, onChanged, toast }: Props) {
  const [clientId, setClientId] = useState(settings.xClientId);
  const [clientSecret, setClientSecret] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState(settings.claudeModel);
  const [connecting, setConnecting] = useState(false);
  const [pending, setPending] = useState<number | null>(null);
  const [tagging, setTagging] = useState(false);
  const [downloading, setDownloading] = useState(false);

  useEffect(() => {
    api.countPendingTags().then(setPending).catch(() => undefined);
  }, [settings]);

  const save = async (patch: SettingsPatch, message?: string) => {
    try {
      onSettings(await api.saveSettings(patch));
      if (message) toast("success", message);
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  const connect = async () => {
    setConnecting(true);
    try {
      if (clientId.trim() !== settings.xClientId || clientSecret) {
        await api.saveSettings({ xClientId: clientId, ...(clientSecret ? { xClientSecret: clientSecret } : {}) });
        setClientSecret("");
      }
      toast("info", "ブラウザで X の認可画面を開きました。「アプリにアクセスを許可」を押してください。");
      const s = await api.xConnect();
      onSettings(s);
      toast("success", `@${s.xUsername} と連携しました。「同期」でいいね・ブックマークを取得できます。`);
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setConnecting(false);
    }
  };

  const tagAll = async () => {
    setTagging(true);
    try {
      const r = await api.tagPending();
      toast(r.failed ? "error" : "success", r.failed ? `${r.tagged} 件をタグ付け（失敗 ${r.failed} 件）\n${r.errors.slice(0, 3).join("\n")}` : `${r.tagged} 件をタグ付けしました`);
      onChanged();
      setPending(await api.countPendingTags());
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setTagging(false);
    }
  };

  const downloadAll = async () => {
    setDownloading(true);
    try {
      const n = await api.downloadMedia();
      toast("success", n ? `画像 ${n} 件を保存しました` : "新しく保存する画像はありませんでした");
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setDownloading(false);
    }
  };

  const copy = (text: string) => {
    navigator.clipboard?.writeText(text).then(
      () => toast("success", "コピーしました"),
      () => toast("error", "コピーできませんでした"),
    );
  };

  return (
    <>
      <div className="page-head">
        <div>
          <h1 className="page-title">設定</h1>
          <div className="page-sub">X との連携、Claude による自動タグ付け、表示の設定</div>
        </div>
      </div>

      <div className="settings">
        {/* ------------------------------------------------------------ X */}
        <section className="panel">
          <div className="panel-head">
            <div className="panel-icon">
              <AtSign />
            </div>
            <div style={{ flex: 1 }}>
              <div className="panel-title">X 連携</div>
              <div className="panel-desc">いいね・ブックマークを公式 API（従量課金）で取得します</div>
            </div>
            {settings.xConnected ? (
              <span className="status-pill ok">
                <span className="dot" /> 連携中
              </span>
            ) : (
              <span className="status-pill off">
                <span className="dot" /> 未連携
              </span>
            )}
          </div>
          <div className="panel-body">
            {settings.xConnected ? (
              <div className="account">
                <img className="avatar" src={mediaSrc(null, settings.xAvatar) ?? "/icon.svg"} alt="" />
                <div className="account-text">
                  <div className="account-name">{settings.xName}</div>
                  <div className="account-sub">
                    @{settings.xUsername}
                    {settings.lastSyncAt ? ` · 最終同期 ${longDate(settings.lastSyncAt)}` : " · まだ同期していません"}
                  </div>
                  {settings.lastSyncSummary && <div className="account-sub">{settings.lastSyncSummary}</div>}
                </div>
                <button className="btn btn-sm btn-ghost" onClick={async () => onSettings(await api.xDisconnect())}>
                  <LogOut /> 連携解除
                </button>
              </div>
            ) : (
              <ol className="steps">
                <li>
                  <a
                    className="link"
                    href="https://developer.x.com/en/portal/dashboard"
                    onClick={(e) => {
                      e.preventDefault();
                      openExternal("https://developer.x.com/en/portal/dashboard");
                    }}
                  >
                    X の開発者ポータル
                  </a>
                  でアプリを作り、クレジットをチャージする
                </li>
                <li>
                  「User authentication settings」で OAuth 2.0 を有効にし、種類は <b>Native App</b>（公開クライアント）を選ぶ
                </li>
                <li>
                  Callback URI に <code>{settings.xRedirectUri}</code> を登録する
                  <button className="icon-btn small" onClick={() => copy(settings.xRedirectUri)} aria-label="コピー" style={{ verticalAlign: "middle" }}>
                    <Copy />
                  </button>
                </li>
                <li>表示された OAuth 2.0 の Client ID を下に貼り付けて「連携する」</li>
              </ol>
            )}

            <div className="row">
              <div className="row-label">
                Client ID
                <div className="row-hint">OAuth 2.0 Client ID</div>
              </div>
              <input className="field mono" value={clientId} onChange={(e) => setClientId(e.target.value)} placeholder="例: aBcD1234efGh..." />
            </div>
            <div className="row">
              <div className="row-label">
                Client Secret
                <div className="row-hint">Web App 型のときだけ必要</div>
              </div>
              <input
                className="field mono"
                type="password"
                value={clientSecret}
                onChange={(e) => setClientSecret(e.target.value)}
                placeholder={settings.hasXClientSecret ? "設定済み（変更するときだけ入力）" : "Native App なら空のまま"}
              />
            </div>
            <div className="inline">
              <button className="btn btn-primary" onClick={connect} disabled={connecting || !clientId.trim()}>
                {connecting ? <LoaderCircle className="spin" /> : <AtSign />}
                {connecting ? "ブラウザで許可を待っています…" : settings.xConnected ? "再連携する" : "連携する"}
              </button>
              {settings.hasXClientSecret && (
                <button className="btn btn-ghost" onClick={() => save({ xClientSecret: "" }, "Client Secret を削除しました")}>
                  Secret を削除
                </button>
              )}
            </div>

            <div className="row">
              <div className="row-label">取得するもの</div>
              <div className="inline" style={{ gap: 18 }}>
                <label className="inline">
                  <Switch on={settings.syncLikes} onChange={(v) => save({ syncLikes: v })} label="いいね" /> いいね
                </label>
                <label className="inline">
                  <Switch on={settings.syncBookmarks} onChange={(v) => save({ syncBookmarks: v })} label="ブックマーク" /> ブックマーク
                </label>
              </div>
            </div>
            <div className="row">
              <div className="row-label">
                毎日自動で同期
                <div className="row-hint">アプリを開いている間、前回から 20 時間たつと同期</div>
              </div>
              <Switch on={settings.autoSync} onChange={(v) => save({ autoSync: v })} label="自動同期" />
            </div>
            <div className="row">
              <div className="row-label">
                1 回の上限
                <div className="row-hint">種類ごとに取得する最大件数</div>
              </div>
              <div className="inline">
                <input
                  className="field"
                  type="number"
                  min={10}
                  max={100000}
                  step={100}
                  style={{ width: 130 }}
                  defaultValue={settings.syncMaxItems}
                  onBlur={(e) => {
                    const v = Number(e.target.value);
                    if (v && v !== settings.syncMaxItems) save({ syncMaxItems: v }, "保存しました");
                  }}
                />
                <span className="muted">件（目安: 1,000 件で約 $1）</span>
              </div>
            </div>
            <div className="inline">
              <button className="btn" onClick={() => onSync(false)} disabled={!settings.xConnected || syncing}>
                <RefreshCw className={syncing ? "spin" : undefined} /> 今すぐ同期
              </button>
              <button className="btn btn-ghost" onClick={() => onSync(true)} disabled={!settings.xConnected || syncing} title="上限まで全件を取り直します（課金されます）">
                全件を取り直す
              </button>
            </div>
            <div className="callout">
              <Info />
              <div>
                自分のいいね・ブックマークの取得は 1 件あたり約 $0.001 です。2 回目以降は新着だけを取りに行き、最初は 20 件だけ確認するので、毎日の同期は数円程度です。X 側のブックマーク API は新しいほうから 800 件までしか返さないことがあります。
              </div>
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------------ Claude */}
        <section className="panel">
          <div className="panel-head">
            <div className="panel-icon">
              <Bot />
            </div>
            <div style={{ flex: 1 }}>
              <div className="panel-title">Claude で自動タグ付け</div>
              <div className="panel-desc">保存したものに Claude がタグと一行要約を付けます（既存のタグを優先して使います）</div>
            </div>
            {settings.hasAnthropicKey ? (
              <span className="status-pill ok">
                <span className="dot" /> {settings.anthropicKeyFromEnv ? "環境変数のキー" : "キー設定済み"}
              </span>
            ) : (
              <span className="status-pill off">
                <span className="dot" /> キー未設定
              </span>
            )}
          </div>
          <div className="panel-body">
            <div className="row">
              <div className="row-label">
                <KeyRound size={13} style={{ verticalAlign: -1, marginRight: 4 }} />
                API キー
                <div className="row-hint">console.anthropic.com で発行</div>
              </div>
              <div className="inline" style={{ flexWrap: "nowrap" }}>
                <input
                  className="field mono"
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder={settings.hasAnthropicKey ? "設定済み（変更するときだけ入力）" : "sk-ant-..."}
                />
                <button
                  className="btn"
                  disabled={!apiKey.trim()}
                  onClick={async () => {
                    await save({ anthropicApiKey: apiKey }, "API キーを保存しました");
                    setApiKey("");
                  }}
                >
                  保存
                </button>
                {settings.hasAnthropicKey && !settings.anthropicKeyFromEnv && (
                  <button className="btn btn-ghost" onClick={() => save({ anthropicApiKey: "" }, "API キーを削除しました")}>
                    削除
                  </button>
                )}
              </div>
            </div>
            <div className="row">
              <div className="row-label">
                モデル
                <div className="row-hint">既定は claude-opus-5-5</div>
              </div>
              <div className="inline" style={{ flexWrap: "nowrap" }}>
                <input className="field mono" value={model} onChange={(e) => setModel(e.target.value)} list="claude-models" />
                <datalist id="claude-models">
                  <option value="claude-opus-5-5" />
                  <option value="claude-sonnet-5-5" />
                  <option value="claude-haiku-4-5" />
                </datalist>
                <button className="btn" disabled={model.trim() === settings.claudeModel} onClick={() => save({ claudeModel: model }, "モデルを変更しました")}>
                  保存
                </button>
              </div>
            </div>
            <div className="row">
              <div className="row-label">
                保存時に自動でタグ付け
                <div className="row-hint">同期・URL 追加のあとに実行</div>
              </div>
              <Switch on={settings.autoTag} onChange={(v) => save({ autoTag: v })} label="自動タグ付け" />
            </div>
            <div className="inline">
              <button className="btn btn-primary" onClick={tagAll} disabled={!settings.hasAnthropicKey || tagging || !pending}>
                {tagging ? <LoaderCircle className="spin" /> : <Sparkles />}
                {pending ? `未処理の ${pending.toLocaleString()} 件をタグ付け` : "未処理のものはありません"}
              </button>
            </div>
            <div className="callout">
              <Info />
              <div>
                記事は本文の先頭 6,000 文字だけを送ります。料金の目安は 1 件あたり数円です（件数が多いときはモデルを <span className="code-inline">claude-haiku-4-5</span> にすると安くなります）。キーはこのパソコンのデータフォルダ内にだけ保存されます。
              </div>
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------------ 表示・データ */}
        <section className="panel">
          <div className="panel-head">
            <div className="panel-icon">
              <SlidersHorizontal />
            </div>
            <div>
              <div className="panel-title">表示とデータ</div>
              <div className="panel-desc">ピックアップの件数、画像の保存、データの場所</div>
            </div>
          </div>
          <div className="panel-body">
            <div className="row">
              <div className="row-label">
                ピックアップの件数
                <div className="row-hint">毎日ランダムに表示する数</div>
              </div>
              <input
                className="field"
                type="number"
                min={1}
                max={50}
                style={{ width: 100 }}
                defaultValue={settings.pickupCount}
                onBlur={(e) => {
                  const v = Number(e.target.value);
                  if (v && v !== settings.pickupCount) save({ pickupCount: v }, "保存しました（明日から、または「引き直す」で反映）");
                }}
              />
            </div>
            <div className="row">
              <div className="row-label">
                画像をローカルに保存
                <div className="row-hint">元のポストが消えても画像が残ります</div>
              </div>
              <div className="inline">
                <Switch on={settings.downloadMedia} onChange={(v) => save({ downloadMedia: v })} label="画像を保存" />
                <button className="btn btn-sm" onClick={downloadAll} disabled={!settings.downloadMedia || downloading}>
                  {downloading ? <LoaderCircle className="spin" /> : <Download />} まだの画像を保存
                </button>
              </div>
            </div>
            <div className="row">
              <div className="row-label">データの場所</div>
              <div className="inline" style={{ flexWrap: "nowrap" }}>
                <code className="code-inline" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {settings.dataDir}
                </code>
                <button className="btn btn-sm" onClick={() => api.openDataDir().catch((e) => toast("error", errorMessage(e)))}>
                  <FolderOpen /> 開く
                </button>
              </div>
            </div>
          </div>
        </section>
      </div>
    </>
  );
}
