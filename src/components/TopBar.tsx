import { forwardRef } from "react";
import { LoaderCircle, Plus, RefreshCw, Search, X } from "lucide-react";
import type { Progress } from "../types";

interface Props {
  keyword: string;
  onKeyword: (v: string) => void;
  onAdd: () => void;
  onSync: () => void;
  syncing: boolean;
  canSync: boolean;
  progress: Progress | null;
}

export const TopBar = forwardRef<HTMLInputElement, Props>(function TopBar({ keyword, onKeyword, onAdd, onSync, syncing, canSync, progress }, ref) {
  const pct = progress && progress.total > 0 ? Math.min(100, (progress.current / progress.total) * 100) : 0;
  return (
    <header className="topbar">
      <div className="search">
        <Search className="icon" />
        <input
          ref={ref}
          value={keyword}
          onChange={(e) => onKeyword(e.target.value)}
          placeholder="検索（スペースで AND・-語 で除外）"
          aria-label="キーワード検索"
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              onKeyword("");
              (e.target as HTMLInputElement).blur();
            }
          }}
        />
        {keyword && (
          <button className="icon-btn small clear" onClick={() => onKeyword("")} aria-label="検索語を消す">
            <X />
          </button>
        )}
        <span className="kbd">/</span>
      </div>

      <span className="topbar-spacer" />

      {progress && (
        <div className="progress-pill" title={progress.message}>
          {progress.done ? null : <LoaderCircle className="spin" />}
          <span>{progress.message}</span>
          {!progress.done && <i className="bar" style={{ width: `${pct}%` }} />}
        </div>
      )}

      {canSync && (
        <button className="btn" onClick={onSync} disabled={syncing} title="X のいいね・ブックマークを取得">
          <RefreshCw className={syncing ? "spin" : undefined} />
          同期
        </button>
      )}
      <button className="btn btn-primary" onClick={onAdd}>
        <Plus />
        追加
      </button>
    </header>
  );
});
