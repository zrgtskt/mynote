import { useCallback, useEffect, useRef, useState } from "react";
import { Search, X } from "lucide-react";
import { api, errorMessage } from "../api";
import { ItemCard } from "../components/ItemCard";
import { EmptyState } from "../components/common";
import type { Filter, Item, Sort, TagMode } from "../types";

const PAGE = 60;

const FILTER_TITLES: Record<Filter, string> = {
  all: "すべて",
  liked: "いいね",
  bookmarked: "ブックマーク",
  tweet: "X のポスト",
  article: "Web 記事",
  manual: "手動で追加",
  untagged: "タグなし",
};

interface Props {
  refreshKey: number;
  keyword: string;
  terms: string[];
  tags: string[];
  tagMode: TagMode;
  filter: Filter;
  sort: Sort;
  onSort: (s: Sort) => void;
  onTagMode: (m: TagMode) => void;
  onRemoveTag: (t: string) => void;
  onClearKeyword: () => void;
  onClearFilter: () => void;
  onOpen: (id: number) => void;
  onTag: (tag: string) => void;
  toast: (kind: "success" | "error" | "info", message: string) => void;
}

export function ItemsView(p: Props) {
  const { refreshKey, keyword, tags, tagMode, filter, sort, toast } = p;
  const [items, setItems] = useState<Item[] | null>(null);
  const [total, setTotal] = useState(0);
  const [loadingMore, setLoadingMore] = useState(false);
  const sentinel = useRef<HTMLDivElement>(null);
  const countRef = useRef(0);
  const queryKey = JSON.stringify({ keyword, tags, tagMode, filter, sort });
  const lastQueryKey = useRef("");

  // 条件が変わったら最初から、データが更新されたら今読み込んでいる件数ぶん読み直す
  useEffect(() => {
    let alive = true;
    const sameQuery = lastQueryKey.current === queryKey;
    lastQueryKey.current = queryKey;
    const limit = sameQuery ? Math.max(PAGE, countRef.current) : PAGE;
    if (!sameQuery) setItems(null);
    api
      .listItems({ keyword, tags, tagMode, filter, sort, limit, offset: 0 })
      .then((page) => {
        if (!alive) return;
        setItems(page.items);
        setTotal(page.total);
        countRef.current = page.items.length;
      })
      .catch((e) => toast("error", errorMessage(e)));
    return () => {
      alive = false;
    };
  }, [queryKey, refreshKey]);

  const loadMore = useCallback(async () => {
    if (!items || loadingMore || items.length >= total || sort === "random") return;
    setLoadingMore(true);
    try {
      const page = await api.listItems({ keyword, tags, tagMode, filter, sort, limit: PAGE, offset: items.length });
      setItems((cur) => {
        const next = [...(cur ?? []), ...page.items.filter((n) => !(cur ?? []).some((c) => c.id === n.id))];
        countRef.current = next.length;
        return next;
      });
      setTotal(page.total);
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setLoadingMore(false);
    }
  }, [items, loadingMore, total, keyword, tags, tagMode, filter, sort, toast]);

  useEffect(() => {
    const el = sentinel.current;
    if (!el) return;
    const io = new IntersectionObserver((entries) => entries[0]?.isIntersecting && loadMore(), { rootMargin: "800px" });
    io.observe(el);
    return () => io.disconnect();
  }, [loadMore]);

  const title = keyword ? `「${keyword}」の検索結果` : tags.length ? tags.map((t) => `#${t}`).join(" ") : FILTER_TITLES[filter];
  const hasConditions = !!keyword || tags.length > 0 || filter !== "all";

  return (
    <>
      <div className="page-head">
        <div>
          <h1 className="page-title">{title}</h1>
          <div className="page-sub">{items ? `${total.toLocaleString()} 件` : "読み込み中…"}</div>
        </div>
        <div className="page-actions">
          <select className="select" value={sort} onChange={(e) => p.onSort(e.target.value as Sort)} aria-label="並び順">
            <option value="saved_desc">保存が新しい順</option>
            <option value="saved_asc">保存が古い順</option>
            <option value="published_desc">投稿日が新しい順</option>
            <option value="random">ランダム</option>
          </select>
        </div>
      </div>

      {hasConditions && (
        <div className="filter-bar">
          <span className="filter-label">絞り込み:</span>
          {filter !== "all" && (
            <span className="chip chip-active">
              {FILTER_TITLES[filter]}
              <button className="chip-x" onClick={p.onClearFilter} aria-label="絞り込みを外す">
                <X />
              </button>
            </span>
          )}
          {keyword && (
            <span className="chip chip-active">
              <Search size={12} /> {keyword}
              <button className="chip-x" onClick={p.onClearKeyword} aria-label="検索語を消す">
                <X />
              </button>
            </span>
          )}
          {tags.map((t) => (
            <span key={t} className="chip chip-active">
              <span className="hash">#</span>
              {t}
              <button className="chip-x" onClick={() => p.onRemoveTag(t)} aria-label={`${t} を外す`}>
                <X />
              </button>
            </span>
          ))}
          {tags.length > 1 && (
            <div className="segmented" role="group" aria-label="タグの組み合わせ">
              <button className={tagMode === "and" ? "on" : ""} onClick={() => p.onTagMode("and")}>
                すべて含む
              </button>
              <button className={tagMode === "or" ? "on" : ""} onClick={() => p.onTagMode("or")}>
                いずれか
              </button>
            </div>
          )}
        </div>
      )}

      {!items ? (
        <div className="masonry">
          {Array.from({ length: 9 }, (_, i) => (
            <div key={i} className="skeleton" style={{ height: 140 + ((i * 57) % 120) }} />
          ))}
        </div>
      ) : items.length === 0 ? (
        <EmptyState icon={<Search />} title="見つかりませんでした">
          {keyword ? "別のキーワードや、タグでの絞り込みを試してください。" : "この条件に当てはまるものはまだありません。"}
        </EmptyState>
      ) : (
        <>
          <div className="masonry">
            {items.map((it, i) => (
              <ItemCard
                key={it.id}
                item={it}
                terms={p.terms}
                activeTags={tags}
                onOpen={p.onOpen}
                onTag={p.onTag}
                style={{ animationDelay: `${Math.min(i % PAGE, 20) * 25}ms` }}
              />
            ))}
          </div>
          <div ref={sentinel} className="sentinel" />
          {items.length < total && sort !== "random" && <div className="load-more">{loadingMore ? "読み込み中…" : ""}</div>}
        </>
      )}
    </>
  );
}
