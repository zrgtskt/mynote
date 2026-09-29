import { AtSign, Bookmark, Heart, Inbox, LayoutGrid, Link2, MessageSquareText, Newspaper, Settings as SettingsIcon, Sparkles, Tags } from "lucide-react";
import type { ReactNode } from "react";
import { mediaSrc } from "../api";
import { relativeTime } from "../lib/format";
import type { Filter, Settings, Stats, TagCount } from "../types";

interface Props {
  view: string;
  filter: Filter;
  activeTags: string[];
  hasQuery: boolean;
  stats: Stats | null;
  tags: TagCount[];
  settings: Settings | null;
  onPickup: () => void;
  onFilter: (f: Filter) => void;
  onTag: (tag: string) => void;
  onTags: () => void;
  onSettings: () => void;
}

function NavItem({ icon, label, count, active, onClick }: { icon: ReactNode; label: string; count?: number; active?: boolean; onClick: () => void }) {
  return (
    <button className={`nav-item${active ? " active" : ""}`} onClick={onClick}>
      <span className="nav-icon">{icon}</span>
      {label}
      {count != null && <span className="nav-count">{count.toLocaleString()}</span>}
    </button>
  );
}

export function Sidebar({ view, filter, activeTags, hasQuery, stats, tags, settings, onPickup, onFilter, onTag, onTags, onSettings }: Props) {
  const inItems = view === "items";
  const isFilter = (f: Filter) => inItems && filter === f && !hasQuery;
  const activeLower = activeTags.map((t) => t.toLowerCase());

  return (
    <nav className="sidebar">
      <div className="brand">
        <img src="/icon.svg" alt="" />
        <div>
          <div className="brand-name">mynote</div>
          <div className="brand-sub">保存したものを、もう一度。</div>
        </div>
      </div>

      <div className="sidebar-scroll">
        <NavItem icon={<Sparkles size={17} />} label="今日のピックアップ" active={view === "pickup"} onClick={onPickup} />
        <NavItem icon={<LayoutGrid size={17} />} label="すべて" count={stats?.total} active={isFilter("all")} onClick={() => onFilter("all")} />

        <div className="nav-label">コレクション</div>
        <NavItem icon={<Heart size={17} />} label="いいね" count={stats?.liked} active={isFilter("liked")} onClick={() => onFilter("liked")} />
        <NavItem icon={<Bookmark size={17} />} label="ブックマーク" count={stats?.bookmarked} active={isFilter("bookmarked")} onClick={() => onFilter("bookmarked")} />
        <NavItem icon={<MessageSquareText size={17} />} label="ポスト" count={stats?.tweets} active={isFilter("tweet")} onClick={() => onFilter("tweet")} />
        <NavItem icon={<Newspaper size={17} />} label="Web 記事" count={stats?.articles} active={isFilter("article")} onClick={() => onFilter("article")} />
        <NavItem icon={<Link2 size={17} />} label="手動で追加" count={stats?.manual} active={isFilter("manual")} onClick={() => onFilter("manual")} />
        <NavItem icon={<Inbox size={17} />} label="タグなし" count={stats?.untagged} active={isFilter("untagged")} onClick={() => onFilter("untagged")} />

        <div className="nav-label">
          <span>タグ</span>
        </div>
        <NavItem icon={<Tags size={17} />} label="タグ一覧" count={stats?.tags} active={view === "tags"} onClick={onTags} />
        {tags.slice(0, 30).map((t) => (
          <button key={t.name} className={`nav-item nav-tag${inItems && activeLower.includes(t.name.toLowerCase()) ? " active" : ""}`} onClick={() => onTag(t.name)}>
            <span className="hash">#</span>
            <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{t.name}</span>
            <span className="nav-count">{t.count}</span>
          </button>
        ))}
      </div>

      <div className="sidebar-footer">
        <NavItem icon={<SettingsIcon size={17} />} label="設定" active={view === "settings"} onClick={onSettings} />
        <div className="account" style={{ marginTop: 8, cursor: "pointer" }} onClick={onSettings}>
          {settings?.xConnected ? (
            <img className="avatar" src={mediaSrc(null, settings.xAvatar) ?? "/icon.svg"} alt="" style={{ width: 30, height: 30 }} />
          ) : (
            <div className="avatar avatar-fallback" style={{ width: 30, height: 30 }}>
              <AtSign size={15} />
            </div>
          )}
          <div className="account-text">
            <div className="account-name">{settings?.xConnected ? settings.xName ?? `@${settings.xUsername}` : "X と未連携"}</div>
            <div className="account-sub">
              {settings?.xConnected
                ? settings.lastSyncAt
                  ? `同期: ${relativeTime(settings.lastSyncAt)}`
                  : "まだ同期していません"
                : "設定から連携できます"}
            </div>
          </div>
        </div>
      </div>
    </nav>
  );
}
