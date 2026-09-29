import { useMemo, useState } from "react";
import { Check, Pencil, Search, Tags, Trash2, X } from "lucide-react";
import { api, errorMessage } from "../api";
import { EmptyState } from "../components/common";
import type { TagCount } from "../types";

interface Props {
  tags: TagCount[];
  onTag: (tag: string) => void;
  onChanged: () => void;
  toast: (kind: "success" | "error" | "info", message: string) => void;
}

function TagTile({ tag, max, onOpen, onChanged, toast }: { tag: TagCount; max: number; onOpen: () => void; onChanged: () => void; toast: Props["toast"] }) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(tag.name);
  const [confirm, setConfirm] = useState(false);

  const rename = async () => {
    const to = name.trim();
    setEditing(false);
    if (!to || to === tag.name) return;
    try {
      await api.renameTag(tag.name, to);
      toast("success", `「${tag.name}」を「${to}」に変更しました`);
      onChanged();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  const remove = async () => {
    if (!confirm) {
      setConfirm(true);
      return;
    }
    try {
      await api.deleteTag(tag.name);
      toast("success", `タグ「${tag.name}」を削除しました`);
      onChanged();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  return (
    <div className="tag-tile" onClick={() => !editing && onOpen()} tabIndex={0} onKeyDown={(e) => e.key === "Enter" && !editing && onOpen()}>
      <div className="tag-tile-top">
        {editing ? (
          <input
            className="inline-input"
            autoFocus
            value={name}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Enter") rename();
              if (e.key === "Escape") {
                setName(tag.name);
                setEditing(false);
              }
            }}
            onBlur={rename}
          />
        ) : (
          <div className="tag-tile-name" title={tag.name}>
            <span className="hash">#</span>
            {tag.name}
          </div>
        )}
        <div className="tag-actions" onClick={(e) => e.stopPropagation()}>
          {editing ? (
            <button className="icon-btn small" onMouseDown={(e) => e.preventDefault()} onClick={rename} aria-label="確定">
              <Check />
            </button>
          ) : (
            <button className="icon-btn small" onClick={() => setEditing(true)} aria-label="名前を変更" title="名前を変更（既存のタグ名にすると統合）">
              <Pencil />
            </button>
          )}
          <button
            className="icon-btn small"
            style={confirm ? { color: "var(--danger)" } : undefined}
            onClick={remove}
            onBlur={() => setConfirm(false)}
            aria-label="削除"
            title={confirm ? "もう一度押すと削除" : "タグを削除（アイテムは残ります）"}
          >
            {confirm ? <X /> : <Trash2 />}
          </button>
        </div>
      </div>
      <div className="tag-bar">
        <span style={{ width: `${Math.max(4, (tag.count / max) * 100)}%` }} />
      </div>
      <div className="tag-tile-count">{tag.count.toLocaleString()} 件</div>
    </div>
  );
}

export function TagsView({ tags, onTag, onChanged, toast }: Props) {
  const [q, setQ] = useState("");
  const [order, setOrder] = useState<"count" | "name">("count");

  const max = tags[0]?.count ?? 1;
  const filtered = useMemo(() => {
    const list = tags.filter((t) => t.name.toLowerCase().includes(q.trim().toLowerCase()));
    return order === "name" ? [...list].sort((a, b) => a.name.localeCompare(b.name, "ja")) : list;
  }, [tags, q, order]);

  const cloud = useMemo(() => {
    const top = tags.slice(0, 40);
    const counts = top.map((t) => t.count);
    const lo = Math.min(...counts);
    const hi = Math.max(...counts);
    return [...top]
      .sort((a, b) => a.name.localeCompare(b.name, "ja"))
      .map((t) => ({ ...t, size: hi === lo ? 16 : 13 + ((t.count - lo) / (hi - lo)) * 15 }));
  }, [tags]);

  if (!tags.length) {
    return (
      <>
        <div className="page-head">
          <h1 className="page-title">タグ一覧</h1>
        </div>
        <EmptyState icon={<Tags />} title="タグはまだありません">
          アイテムを開いてタグを付けるか、設定で Claude の自動タグ付けを有効にしてください。
        </EmptyState>
      </>
    );
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1 className="page-title">タグ一覧</h1>
          <div className="page-sub">{tags.length.toLocaleString()} 個のタグ。クリックで絞り込み、鉛筆で名前の変更（既存のタグ名にすると統合）。</div>
        </div>
        <div className="page-actions">
          <div className="search" style={{ width: 240 }}>
            <Search className="icon" />
            <input style={{ height: 34, paddingRight: 12 }} value={q} onChange={(e) => setQ(e.target.value)} placeholder="タグを絞り込み" />
          </div>
          <div className="segmented">
            <button className={order === "count" ? "on" : ""} onClick={() => setOrder("count")}>
              件数順
            </button>
            <button className={order === "name" ? "on" : ""} onClick={() => setOrder("name")}>
              名前順
            </button>
          </div>
        </div>
      </div>

      {!q && (
        <div className="tag-cloud">
          {cloud.map((t) => (
            <button key={t.name} className="cloud-tag" style={{ fontSize: t.size, opacity: 0.6 + (t.size - 13) / 40 }} onClick={() => onTag(t.name)}>
              #{t.name}
            </button>
          ))}
        </div>
      )}

      <div className="tag-grid">
        {filtered.map((t) => (
          <TagTile key={t.name} tag={t} max={max} onOpen={() => onTag(t.name)} onChanged={onChanged} toast={toast} />
        ))}
      </div>
    </>
  );
}
