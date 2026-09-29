import { useEffect, useState } from "react";
import { Check, ExternalLink, LoaderCircle, MessageSquareText, Newspaper, Repeat2, Heart, Sparkles, StickyNote, Tag, Trash2, X } from "lucide-react";
import { api, errorMessage, mediaSrc } from "../api";
import { compactNumber, hostOf, longDate } from "../lib/format";
import type { ItemDetail, TagCount } from "../types";
import { Avatar, RichText, SiteBadge, openExternal } from "./common";
import { Sources } from "./ItemCard";
import { TagEditor } from "./TagEditor";

interface Props {
  id: number;
  allTags: TagCount[];
  canRetag: boolean;
  onClose: () => void;
  onChanged: () => void;
  onDeleted: () => void;
  onTag: (tag: string) => void;
  toast: (kind: "success" | "error" | "info", message: string) => void;
}

export function ItemDrawer({ id, allTags, canRetag, onClose, onChanged, onDeleted, onTag, toast }: Props) {
  const [item, setItem] = useState<ItemDetail | null>(null);
  const [note, setNote] = useState("");
  const [retagging, setRetagging] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => {
    let alive = true;
    setItem(null);
    setConfirmDelete(false);
    api
      .getItem(id)
      .then((it) => {
        if (!alive) return;
        setItem(it);
        setNote(it?.note ?? "");
      })
      .catch((e) => toast("error", errorMessage(e)));
    return () => {
      alive = false;
    };
  }, [id, toast]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const saveTags = async (tags: string[]) => {
    if (!item) return;
    setItem({ ...item, tags });
    try {
      const saved = await api.setItemTags(item.id, tags);
      setItem((cur) => (cur ? { ...cur, tags: saved } : cur));
      onChanged();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  const saveNote = async () => {
    if (!item || (item.note ?? "") === note) return;
    try {
      await api.setNote(item.id, note);
      setItem({ ...item, note });
      toast("success", "メモを保存しました");
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  const retag = async () => {
    if (!item) return;
    setRetagging(true);
    try {
      const updated = await api.retagItem(item.id);
      setItem(updated);
      toast("success", "Claude でタグを付け直しました");
      onChanged();
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setRetagging(false);
    }
  };

  const remove = async () => {
    if (!item) return;
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    try {
      await api.deleteItem(item.id);
      toast("success", "削除しました");
      onDeleted();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  const isArticle = item?.kind === "article";

  return (
    <>
      <div className="overlay" onClick={onClose} />
      <aside className="drawer" role="dialog" aria-label="詳細">
        <div className="drawer-bar">
          {item && (
            <span className={`kind-badge ${item.kind}`}>
              {isArticle ? <Newspaper /> : <MessageSquareText />}
              {isArticle ? "Web 記事" : "X のポスト"}
            </span>
          )}
          {item && <Sources item={item} />}
          <span style={{ flex: 1 }} />
          {item && (
            <>
              <button className="btn btn-sm btn-ghost" onClick={() => openExternal(item.url)}>
                <ExternalLink /> 元のページ
              </button>
              {canRetag && (
                <button className="btn btn-sm btn-ghost" onClick={retag} disabled={retagging} title="Claude でタグと要約を付け直す">
                  {retagging ? <LoaderCircle className="spin" /> : <Sparkles />} AI タグ
                </button>
              )}
              <button className={`btn btn-sm ${confirmDelete ? "btn-danger" : "btn-ghost"}`} onClick={remove} onBlur={() => setConfirmDelete(false)}>
                {confirmDelete ? <Check /> : <Trash2 />}
                {confirmDelete ? "本当に削除" : "削除"}
              </button>
            </>
          )}
          <button className="icon-btn" onClick={onClose} aria-label="閉じる">
            <X />
          </button>
        </div>

        <div className="drawer-body">
          {!item ? (
            <div style={{ display: "grid", gap: 12 }}>
              <div className="skeleton" style={{ height: 48 }} />
              <div className="skeleton" style={{ height: 180 }} />
              <div className="skeleton" style={{ height: 80 }} />
            </div>
          ) : (
            <>
              <div className="card-head" style={{ marginBottom: 14 }}>
                {isArticle ? (
                  <SiteBadge name={item.siteName ?? hostOf(item.url)} />
                ) : (
                  <Avatar local={item.authorAvatarLocal} remote={item.authorAvatar} name={item.authorName} size="lg" />
                )}
                <div className="card-meta">
                  <div className="card-author" style={{ fontSize: 15 }}>
                    {isArticle ? item.siteName ?? hostOf(item.url) : item.authorName}
                  </div>
                  <div className="card-sub">
                    {isArticle ? item.authorName ?? hostOf(item.url) : `@${item.authorHandle ?? ""}`}
                    {item.publishedAt ? ` · ${longDate(item.publishedAt)}` : ""}
                  </div>
                </div>
              </div>

              {isArticle ? (
                <>
                  <h1 className="detail-title">{item.title ?? item.url}</h1>
                  {mediaSrc(item.imageLocal, item.imageUrl) && <img className="detail-cover" src={mediaSrc(item.imageLocal, item.imageUrl)} alt="" />}
                </>
              ) : (
                <div className="detail-text">
                  <RichText text={item.text} onHashtag={onTag} />
                </div>
              )}

              {!isArticle && item.media.length > 0 && (
                <div className="detail-media">
                  {item.media.map((m, i) =>
                    m.videoUrl ? (
                      <video key={i} src={m.videoUrl} poster={mediaSrc(m.localPath, m.previewUrl)} controls preload="none" />
                    ) : (
                      <img key={i} src={mediaSrc(m.localPath, m.url ?? m.previewUrl)} alt={m.alt ?? ""} />
                    ),
                  )}
                </div>
              )}

              {!isArticle && item.link && (
                <div className="link-card" style={{ cursor: "pointer" }} onClick={() => openExternal(item.link!.url)}>
                  {item.link.image && <img src={item.link.image} alt="" />}
                  <div className="link-card-body" style={item.link.image ? undefined : { paddingLeft: 12 }}>
                    <div className="link-card-host">{hostOf(item.link.url)}</div>
                    <div className="link-card-title">{item.link.title ?? item.link.url}</div>
                    {item.link.description && <div className="muted" style={{ fontSize: 12 }}>{item.link.description}</div>}
                  </div>
                </div>
              )}

              {!isArticle && item.metrics && (
                <div className="inline" style={{ marginTop: 14, gap: 16 }}>
                  {item.metrics.like_count != null && (
                    <span className="metric">
                      <Heart /> {compactNumber(item.metrics.like_count)}
                    </span>
                  )}
                  {item.metrics.retweet_count != null && (
                    <span className="metric">
                      <Repeat2 /> {compactNumber(item.metrics.retweet_count)}
                    </span>
                  )}
                  <span className="metric">保存: {longDate(item.savedAt)}</span>
                </div>
              )}

              {item.summary && (
                <div className="detail-section">
                  <div className="summary-box">
                    <Sparkles />
                    <div>{item.summary}</div>
                  </div>
                </div>
              )}

              <div className="detail-section">
                <div className="section-label">
                  <Tag /> タグ
                </div>
                <TagEditor tags={item.tags} allTags={allTags} onChange={saveTags} />
              </div>

              <div className="detail-section">
                <div className="section-label">
                  <StickyNote /> メモ
                </div>
                <textarea
                  className="note"
                  value={note}
                  placeholder="気づいたこと・あとでやることなど（検索の対象になります）"
                  onChange={(e) => setNote(e.target.value)}
                  onBlur={saveNote}
                />
              </div>

              {isArticle && (
                <div className="detail-section">
                  <div className="section-label">
                    <Newspaper /> 本文
                  </div>
                  {item.contentHtml ? (
                    <div
                      className="prose"
                      // 本文はバックエンドで ammonia によりサニタイズ済み
                      dangerouslySetInnerHTML={{ __html: item.contentHtml }}
                      onClick={(e) => {
                        const a = (e.target as HTMLElement).closest("a");
                        if (a) {
                          e.preventDefault();
                          const href = a.getAttribute("href");
                          if (href) openExternal(href);
                        }
                      }}
                    />
                  ) : (
                    <div className="detail-text" style={{ fontSize: 15 }}>
                      {item.text || <span className="muted">本文を取得できませんでした。元のページで確認してください。</span>}
                    </div>
                  )}
                </div>
              )}
            </>
          )}
        </div>
      </aside>
    </>
  );
}
