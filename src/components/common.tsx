import { useState, type ReactNode } from "react";
import { CircleAlert, CircleCheck, Info, X } from "lucide-react";
import { api, mediaSrc } from "../api";
import { tokenize, displayUrl } from "../lib/format";

export function Avatar({ local, remote, name, size = "md" }: { local?: string | null; remote?: string | null; name?: string | null; size?: "md" | "lg" }) {
  const [failed, setFailed] = useState(false);
  const src = mediaSrc(local, remote);
  const cls = `avatar${size === "lg" ? " lg" : ""}`;
  if (!src || failed) {
    return <div className={`${cls} avatar-fallback`}>{(name ?? "?").trim().slice(0, 1).toUpperCase()}</div>;
  }
  return <img className={cls} src={src} alt="" loading="lazy" onError={() => setFailed(true)} />;
}

export function SiteBadge({ name }: { name?: string | null }) {
  return <div className="site-badge">{(name ?? "W").trim().slice(0, 1).toUpperCase()}</div>;
}

/** 検索語を <mark> で強調する */
function highlight(text: string, terms: string[], keyPrefix: string): ReactNode[] {
  const valid = terms.filter((t) => t && !t.startsWith("-"));
  if (!valid.length) return [text];
  const re = new RegExp(`(${valid.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "gi");
  return text.split(re).map((part, i) =>
    i % 2 === 1 ? (
      <mark className="mark" key={`${keyPrefix}-${i}`}>
        {part}
      </mark>
    ) : (
      part
    ),
  );
}

export function openExternal(url: string) {
  api.openExternal(url).catch(() => undefined);
}

/** ツイート本文。URL・ハッシュタグ・メンションをリンクにする */
export function RichText({
  text,
  terms = [],
  onHashtag,
}: {
  text: string;
  terms?: string[];
  onHashtag?: (tag: string) => void;
}) {
  return (
    <>
      {tokenize(text).map((tok, i) => {
        switch (tok.type) {
          case "url":
            return (
              <a
                key={i}
                className="link"
                href={tok.value}
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  openExternal(tok.value);
                }}
              >
                {displayUrl(tok.value)}
              </a>
            );
          case "hashtag":
            return (
              <span
                key={i}
                className="hashtag"
                role="button"
                onClick={(e) => {
                  if (!onHashtag) return;
                  e.stopPropagation();
                  onHashtag(tok.value.slice(1));
                }}
              >
                {tok.value}
              </span>
            );
          case "mention":
            return (
              <a
                key={i}
                className="link"
                href={`https://x.com/${tok.value.slice(1)}`}
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  openExternal(`https://x.com/${tok.value.slice(1)}`);
                }}
              >
                {tok.value}
              </a>
            );
          default:
            return <span key={i}>{highlight(tok.value, terms, String(i))}</span>;
        }
      })}
    </>
  );
}

export function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label?: string }) {
  return <button type="button" role="switch" aria-checked={on} aria-label={label} className={`switch${on ? " on" : ""}`} onClick={() => onChange(!on)} />;
}

export interface Toast {
  id: number;
  kind: "success" | "error" | "info";
  message: string;
}

export function Toasts({ toasts, onClose }: { toasts: Toast[]; onClose: (id: number) => void }) {
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`}>
          {t.kind === "success" ? <CircleCheck /> : t.kind === "error" ? <CircleAlert /> : <Info />}
          <div style={{ flex: 1, whiteSpace: "pre-wrap" }}>{t.message}</div>
          <button className="icon-btn small" onClick={() => onClose(t.id)} aria-label="閉じる">
            <X />
          </button>
        </div>
      ))}
    </div>
  );
}

export function EmptyState({ icon, title, children, action }: { icon: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action}
    </div>
  );
}
