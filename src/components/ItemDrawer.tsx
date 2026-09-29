import { useEffect, useState } from "react";
import { BookOpen, Check, ExternalLink, Heart, LoaderCircle, MessageSquareText, Newspaper, Repeat2, RotateCw, Sparkles, StickyNote, Tag, Trash2, X } from "lucide-react";
import { api, errorMessage, mediaSrc } from "../api";
import { compactNumber, hostOf, longDate } from "../lib/format";
import type { ItemDetail, TagCount } from "../types";
import { Avatar, RichText, SiteBadge, openExternal } from "./common";
import { LinkPreview, Sources } from "./ItemCard";
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
  const [refetching, setRefetching] = useState(false);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    let alive = true;
    setItem(null);
    setConfirmDelete(false);
    setExpanded(false);
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

  const refetchLinked = async () => {
    if (!item) return;
    setRefetching(true);
    try {
      const updated = await api.refetchLinked(item.id);
      setItem(updated);
      toast("success", "紹介先のページを保存しました");
      onChanged();
    } catch (e) {
      toast("error", errorMessage(e));
      // 失敗の理由を表示するため読み直す
      api.getItem(item.id).then((it) => it && setItem(it)).catch(() => undefined);
    } finally {
      setRefetching(false);
    }
  };

  /** 記事本文内のリンクはアプリ内で開かず、ブラウザで開く */
  const openLinksExternally = (e: React.MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (a) {
      e.preventDefault();
      const href = a.getAttribute("href");
      if (href) openExternal(href);
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
                <div style={{ cursor: "pointer" }}>
                  <LinkPreview link={item.link} linked={item.linked} onClick={() => openExternal(item.link!.url)} />
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

              {!isArticle && item.link && (
                <div className="detail-section">
                  <div className="section-label">
                    <BookOpen /> 紹介先の記事
                  </div>
                  {item.linked?.hasContent ? (
                    <div className="linked-box">
                      <div className="linked-head">
                        {mediaSrc(item.linked.imageLocal, item.linked.imageUrl) && <img src={mediaSrc(item.linked.imageLocal, item.linked.imageUrl)} alt="" />}
                        <div style={{ minWidth: 0 }}>
                          <div className="linked-title">{item.linked.title ?? item.link.title ?? item.linked.url}</div>
                          <div className="card-sub">
                            {[item.linked.siteName ?? hostOf(item.linked.url), item.linked.author, item.linked.publishedAt ? longDate(item.linked.publishedAt) : null]
                              .filter(Boolean)
                              .join(" · ")}
                          </div>
                          <div className="card-sub">保存: {longDate(item.linked.fetchedAt)}</div>
                        </div>
                      </div>
                      <div className={`linked-body${expanded ? " expanded" : ""}`}>
                        {item.linkedHtml ? (
                          // 本文はバックエンドで ammonia によりサニタイズ済み
                          <div className="prose" dangerouslySetInnerHTML={{ __html: item.linkedHtml }} onClick={openLinksExternally} />
                        ) : (
                          <div className="detail-text" style={{ fontSize: 15 }}>
                            {item.linkedText}
                          </div>
                        )}
                      </div>
                      <div className="linked-foot">
                        <button className="btn btn-sm btn-ghost" onClick={() => setExpanded((v) => !v)}>
                          {expanded ? "折りたたむ" : "全文を表示"}
                        </button>
                        <button className="btn btn-sm btn-ghost" onClick={() => openExternal(item.linked!.url)}>
                          <ExternalLink /> 元のページ
                        </button>
                        <button className="btn btn-sm btn-ghost" onClick={refetchLinked} disabled={refetching} title="最新の内容で保存し直す">
                          {refetching ? <LoaderCircle className="spin" /> : <RotateCw />} 取り直す
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="callout">
                      <BookOpen />
                      <div style={{ flex: 1 }}>
                        {item.linked?.error ? (
                          <>
                            紹介先のページを保存できませんでした。
                            <div className="muted" style={{ wordBreak: "break-all" }}>{item.linked.error}</div>
                          </>
                        ) : (
                          "紹介先のページはまだ保存していません。"
                        )}
                        <div style={{ marginTop: 8 }}>
                          <button className="btn btn-sm" onClick={refetchLinked} disabled={refetching}>
                            {refetching ? <LoaderCircle className="spin" /> : <RotateCw />}
                            {item.linked?.error ? "もう一度試す" : "今すぐ保存する"}
                          </button>
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              )}

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
                      onClick={openLinksExternally}
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
