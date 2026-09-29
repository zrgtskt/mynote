import { useCallback, useEffect, useState } from "react";
import { Dices, Plus, Settings as SettingsIcon, Sparkles } from "lucide-react";
import { api, errorMessage } from "../api";
import { ItemCard } from "../components/ItemCard";
import { EmptyState } from "../components/common";
import { todayLabel } from "../lib/format";
import type { DailyPickup, Stats } from "../types";

interface Props {
  refreshKey: number;
  stats: Stats | null;
  onOpen: (id: number) => void;
  onTag: (tag: string) => void;
  onAdd: () => void;
  onSettings: () => void;
  toast: (kind: "success" | "error" | "info", message: string) => void;
}

export function PickupView({ refreshKey, stats, onOpen, onTag, onAdd, onSettings, toast }: Props) {
  const [pickup, setPickup] = useState<DailyPickup | null>(null);
  const [shuffling, setShuffling] = useState(false);

  const load = useCallback(
    async (reshuffle = false) => {
      try {
        setPickup(await api.dailyPickup(reshuffle));
      } catch (e) {
        toast("error", errorMessage(e));
      }
    },
    [toast],
  );

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  const shuffle = async () => {
    setShuffling(true);
    await load(true);
    setShuffling(false);
  };

  const empty = pickup && pickup.items.length === 0;

  return (
    <>
      <section className="hero">
        <div className="hero-row">
          <div>
            <div className="hero-eyebrow">
              <Sparkles /> Daily pickup
            </div>
            <h1 className="hero-title">
              <span className="grad">{todayLabel(pickup?.day)}</span>の {pickup?.items.length ?? "…"} 件
            </h1>
            <div className="hero-sub">保存したまま眠っているポストや記事を、毎日ランダムに選んでいます。</div>
          </div>
          <button className="btn" onClick={shuffle} disabled={shuffling || !!empty}>
            <Dices className={shuffling ? "spin" : undefined} />
            引き直す
          </button>
        </div>
        {stats && (
          <div className="stat-row">
            <div className="stat">
              <div className="stat-num">{stats.total.toLocaleString()}</div>
              <div className="stat-label">保存したもの</div>
            </div>
            <div className="stat">
              <div className="stat-num">{stats.liked.toLocaleString()}</div>
              <div className="stat-label">いいね</div>
            </div>
            <div className="stat">
              <div className="stat-num">{stats.bookmarked.toLocaleString()}</div>
              <div className="stat-label">ブックマーク</div>
            </div>
            <div className="stat">
              <div className="stat-num">{stats.articles.toLocaleString()}</div>
              <div className="stat-label">Web 記事</div>
            </div>
            <div className="stat">
              <div className="stat-num">{stats.tags.toLocaleString()}</div>
              <div className="stat-label">タグ</div>
            </div>
          </div>
        )}
      </section>

      {!pickup ? (
        <div className="pickup-grid">
          {[0, 1, 2].map((i) => (
            <div key={i} className="skeleton" style={{ height: 220 }} />
          ))}
        </div>
      ) : empty ? (
        <EmptyState
          icon={<Sparkles />}
          title="まだ何も保存されていません"
          action={
            <div className="inline" style={{ justifyContent: "center" }}>
              <button className="btn btn-primary" onClick={onSettings}>
                <SettingsIcon /> X と連携する
              </button>
              <button className="btn" onClick={onAdd}>
                <Plus /> URL を追加
              </button>
            </div>
          }
        >
          X のいいね・ブックマークを同期するか、気になった記事の URL を追加すると、ここに毎日ピックアップされます。
        </EmptyState>
      ) : (
        <div className="pickup-grid">
          {pickup.items.map((it, i) => (
            <ItemCard key={it.id} item={it} onOpen={onOpen} onTag={onTag} style={{ animationDelay: `${i * 60}ms` }} />
          ))}
        </div>
      )}
    </>
  );
}
