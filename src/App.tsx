import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage, onItemsChanged, onProgress } from "./api";
import { AddUrlModal } from "./components/AddUrlModal";
import { ItemDrawer } from "./components/ItemDrawer";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { Toasts, type Toast } from "./components/common";
import { ItemsView } from "./views/ItemsView";
import { PickupView } from "./views/PickupView";
import { SettingsView } from "./views/SettingsView";
import { TagsView } from "./views/TagsView";
import type { Filter, Progress, Settings, Sort, Stats, TagCount, TagMode } from "./types";

type ViewName = "pickup" | "items" | "tags" | "settings";

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

export default function App() {
  const [view, setView] = useState<ViewName>("pickup");
  const [filter, setFilter] = useState<Filter>("all");
  const [keyword, setKeyword] = useState("");
  const [tags, setTags] = useState<string[]>([]);
  const [tagMode, setTagMode] = useState<TagMode>("and");
  const [sort, setSort] = useState<Sort>("saved_desc");
  const [stats, setStats] = useState<Stats | null>(null);
  const [tagList, setTagList] = useState<TagCount[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [openId, setOpenId] = useState<number | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [adding, setAdding] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const toastId = useRef(0);

  const debouncedKeyword = useDebounced(keyword.trim(), 250);
  const terms = useMemo(() => debouncedKeyword.split(/[\s　]+/).filter(Boolean), [debouncedKeyword]);

  const toast = useCallback((kind: Toast["kind"], message: string) => {
    const id = ++toastId.current;
    setToasts((t) => [...t.slice(-3), { id, kind, message }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), kind === "error" ? 9000 : 4500);
  }, []);

  const refreshMeta = useCallback(() => {
    api.getStats().then(setStats).catch(() => undefined);
    api.listTags().then(setTagList).catch(() => undefined);
  }, []);

  const refreshAll = useCallback(() => {
    refreshMeta();
    setRefreshKey((k) => k + 1);
  }, [refreshMeta]);

  useEffect(() => {
    refreshMeta();
    api
      .getSettings()
      .then(setSettings)
      .catch((e) => toast("error", errorMessage(e)));
  }, [refreshMeta, toast]);

  // バックエンドからの進捗・更新通知
  useEffect(() => {
    let hideTimer: ReturnType<typeof setTimeout> | undefined;
    const subs = [
      onProgress((p) => {
        setProgress(p);
        clearTimeout(hideTimer);
        if (p.done) {
          hideTimer = setTimeout(() => setProgress(null), 5000);
          if (p.task === "sync") api.getSettings().then(setSettings).catch(() => undefined);
        }
      }),
      onItemsChanged(() => refreshAll()),
    ];
    return () => {
      clearTimeout(hideTimer);
      subs.forEach((s) => s.then((un) => un()).catch(() => undefined));
    };
  }, [refreshAll]);

  // キーボード操作: / か Ctrl/Cmd+K で検索
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName) || target.isContentEditable;
      if ((e.key === "/" && !typing) || (e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey))) {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const scrollTop = () => scrollRef.current?.scrollTo({ top: 0 });

  const go = (v: ViewName) => {
    setView(v);
    scrollTop();
  };

  const onKeyword = (v: string) => {
    setKeyword(v);
    if (v.trim() && view !== "items") go("items");
  };

  const showFilter = (f: Filter) => {
    setFilter(f);
    setTags([]);
    setKeyword("");
    go("items");
  };

  /** タグをクリックしたとき: 一覧でそのタグを絞り込みに加える（一覧以外からならそのタグだけで開く） */
  const addTagFilter = useCallback(
    (tag: string) => {
      setOpenId(null);
      if (view !== "items") {
        setTags([tag]);
        setFilter("all");
        setKeyword("");
      } else {
        setTags((cur) => (cur.some((t) => t.toLowerCase() === tag.toLowerCase()) ? cur : [...cur, tag]));
      }
      setView("items");
      scrollTop();
    },
    [view],
  );

  const sync = async (full = false) => {
    setSyncing(true);
    try {
      const r = await api.syncNow(full);
      const newCount = r.likedNew + r.bookmarksNew;
      if (r.errors.length) toast("error", r.errors.join("\n"));
      else toast("success", newCount ? `新しく ${newCount} 件を保存しました` : "新しいいいね・ブックマークはありませんでした");
      setSettings(await api.getSettings());
      refreshAll();
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setSyncing(false);
    }
  };

  const addUrls = async (urls: string[]) => {
    let ok = 0;
    for (const url of urls) {
      try {
        const item = await api.addUrl(url);
        ok++;
        if (urls.length === 1) {
          toast("success", `「${item.title ?? item.authorName ?? item.url}」を保存しました`);
          setOpenId(item.id);
        }
      } catch (e) {
        toast("error", `${url}\n${errorMessage(e)}`);
      }
    }
    if (urls.length > 1) toast("success", `${ok} / ${urls.length} 件を保存しました`);
    refreshAll();
  };

  return (
    <div className="app">
      <Sidebar
        view={view}
        filter={filter}
        activeTags={tags}
        hasQuery={!!keyword.trim() || tags.length > 0}
        stats={stats}
        tags={tagList}
        settings={settings}
        onPickup={() => go("pickup")}
        onFilter={showFilter}
        onTag={(t) => {
          setTags([t]);
          setFilter("all");
          setKeyword("");
          go("items");
        }}
        onTags={() => go("tags")}
        onSettings={() => go("settings")}
      />

      <main className="main">
        <TopBar
          ref={searchRef}
          keyword={keyword}
          onKeyword={onKeyword}
          onAdd={() => setAdding(true)}
          onSync={() => sync(false)}
          syncing={syncing}
          canSync={!!settings?.xConnected}
          progress={progress}
        />
        <div className="scroll" ref={scrollRef}>
          {view === "pickup" && (
            <PickupView
              refreshKey={refreshKey}
              stats={stats}
              onOpen={setOpenId}
              onTag={addTagFilter}
              onAdd={() => setAdding(true)}
              onSettings={() => go("settings")}
              toast={toast}
            />
          )}
          {view === "items" && (
            <ItemsView
              refreshKey={refreshKey}
              keyword={debouncedKeyword}
              terms={terms}
              tags={tags}
              tagMode={tagMode}
              filter={filter}
              sort={sort}
              onSort={setSort}
              onTagMode={setTagMode}
              onRemoveTag={(t) => setTags((cur) => cur.filter((x) => x !== t))}
              onClearKeyword={() => setKeyword("")}
              onClearFilter={() => setFilter("all")}
              onOpen={setOpenId}
              onTag={addTagFilter}
              toast={toast}
            />
          )}
          {view === "tags" && <TagsView tags={tagList} onTag={addTagFilter} onChanged={refreshAll} toast={toast} />}
          {view === "settings" && settings && (
            <SettingsView settings={settings} onSettings={setSettings} onSync={sync} syncing={syncing} onChanged={refreshAll} toast={toast} />
          )}
        </div>
      </main>

      {openId != null && (
        <ItemDrawer
          id={openId}
          allTags={tagList}
          canRetag={!!settings?.hasAnthropicKey}
          onClose={() => setOpenId(null)}
          onChanged={refreshAll}
          onDeleted={() => {
            setOpenId(null);
            refreshAll();
          }}
          onTag={addTagFilter}
          toast={toast}
        />
      )}

      {adding && <AddUrlModal onClose={() => setAdding(false)} onSubmit={addUrls} />}
      <Toasts toasts={toasts} onClose={(id) => setToasts((t) => t.filter((x) => x.id !== id))} />
    </div>
  );
}
