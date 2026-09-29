import { memo } from "react";
import { Bookmark, BookOpenCheck, Heart, Link2, Play, Sparkles } from "lucide-react";
import { mediaSrc } from "../api";
import { compactNumber, hostOf, relativeTime } from "../lib/format";
import type { Item, LinkCard, LinkedPage } from "../types";
import { Avatar, RichText, SiteBadge, openExternal } from "./common";

interface Props {
  item: Item;
  terms?: string[];
  activeTags?: string[];
  onOpen: (id: number) => void;
  onTag: (tag: string) => void;
  style?: React.CSSProperties;
}

export function Sources({ item }: { item: Item }) {
  return (
    <div className="sources">
      {item.isLiked && (
        <span className="source-dot like" title="いいね">
          <Heart fill="currentColor" />
        </span>
      )}
      {item.isBookmarked && (
        <span className="source-dot bookmark" title="ブックマーク">
          <Bookmark fill="currentColor" />
        </span>
      )}
      {item.isManual && (
        <span className="source-dot manual" title="手動で追加">
          <Link2 />
        </span>
      )}
    </div>
  );
}

/** ポストが紹介しているリンク。紹介先ページを保存済みなら「保存済み」を表示する */
export function LinkPreview({ link, linked, onClick }: { link: LinkCard; linked?: LinkedPage | null; onClick?: (e: React.MouseEvent) => void }) {
  const image = mediaSrc(link.imageLocal ?? linked?.imageLocal, link.image ?? linked?.imageUrl);
  const saved = !!linked?.hasContent;
  return (
    <div className="link-card" onClick={onClick} title={saved ? "紹介先のページはローカルに保存済みです" : link.url}>
      {image && <img src={image} alt="" loading="lazy" />}
      <div className="link-card-body" style={image ? undefined : { paddingLeft: 12 }}>
        <div className="link-card-host">
          {linked?.siteName ?? hostOf(linked?.url ?? link.url)}
          {saved && (
            <span className="saved-badge">
              <BookOpenCheck /> 保存済み
            </span>
          )}
        </div>
        <div className="link-card-title">{link.title ?? linked?.title ?? link.url}</div>
      </div>
    </div>
  );
}

function MediaGrid({ item }: { item: Item }) {
  const media = item.media.slice(0, 4);
  if (!media.length) return null;
  return (
    <div className={`media-grid n${media.length}`}>
      {media.map((m, i) => (
        <div className="media-cell" key={i}>
          <img src={mediaSrc(m.localPath, m.url ?? m.previewUrl)} alt={m.alt ?? ""} loading="lazy" />
          {m.kind !== "photo" && (
            <span className="play-badge">
              <Play fill="currentColor" />
            </span>
          )}
        </div>
      ))}
    </div>
  );
}

function TagChips({ tags, active, onTag }: { tags: string[]; active?: string[]; onTag: (t: string) => void }) {
  const shown = tags.slice(0, 4);
  const activeLower = (active ?? []).map((t) => t.toLowerCase());
  return (
    <>
      {shown.map((t) => (
        <button
          key={t}
          className={`chip${activeLower.includes(t.toLowerCase()) ? " chip-active" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            onTag(t);
          }}
        >
          <span className="hash">#</span>
          {t}
        </button>
      ))}
      {tags.length > shown.length && <span className="chip">+{tags.length - shown.length}</span>}
    </>
  );
}

function ItemCardInner({ item, terms = [], activeTags, onOpen, onTag, style }: Props) {
  const isArticle = item.kind === "article";
  const likes = item.metrics?.like_count;
  const cover = isArticle ? mediaSrc(item.imageLocal, item.imageUrl) : undefined;

  return (
    <article
      className="card"
      style={style}
      tabIndex={0}
      onClick={() => onOpen(item.id)}
      onKeyDown={(e) => {
        if (e.key === "Enter") onOpen(item.id);
      }}
    >
      {cover && (
        <div className="card-cover">
          <img src={cover} alt="" loading="lazy" />
        </div>
      )}

      <div className="card-head">
        {isArticle ? (
          <SiteBadge name={item.siteName ?? hostOf(item.url)} />
        ) : (
          <Avatar local={item.authorAvatarLocal} remote={item.authorAvatar} name={item.authorName ?? item.authorHandle} />
        )}
        <div className="card-meta">
          <div className="card-author">{isArticle ? item.siteName ?? hostOf(item.url) : item.authorName ?? item.authorHandle}</div>
          <div className="card-sub">
            {isArticle
              ? [item.authorName, relativeTime(item.publishedAt ?? item.savedAt)].filter(Boolean).join(" · ")
              : `@${item.authorHandle ?? "unknown"} · ${relativeTime(item.publishedAt ?? item.savedAt)}`}
          </div>
        </div>
        <Sources item={item} />
      </div>

      {isArticle ? (
        <>
          <h3 className="card-title">
            <RichText text={item.title ?? item.url} terms={terms} />
          </h3>
          {item.summary ? (
            <div className="card-summary">
              <Sparkles />
              <span>{item.summary}</span>
            </div>
          ) : (
            item.excerpt && <div className="card-excerpt">{item.excerpt}</div>
          )}
        </>
      ) : (
        <>
          <div className="card-text">
            <RichText text={item.text} terms={terms} onHashtag={onTag} />
          </div>
          <MediaGrid item={item} />
          {!item.media.length && item.link && (
            <LinkPreview
              link={item.link}
              linked={item.linked}
              onClick={(e) => {
                // 保存済みなら詳細（保存した本文）を開き、未保存なら元のページを開く
                if (item.linked?.hasContent) return;
                e.stopPropagation();
                openExternal(item.link!.url);
              }}
            />
          )}
        </>
      )}

      {(item.tags.length > 0 || likes != null) && (
        <div className="card-foot">
          <TagChips tags={item.tags} active={activeTags} onTag={onTag} />
          <span className="spacer" />
          {likes != null && likes > 0 && (
            <span className="metric">
              <Heart /> {compactNumber(likes)}
            </span>
          )}
        </div>
      )}
    </article>
  );
}

export const ItemCard = memo(ItemCardInner);
