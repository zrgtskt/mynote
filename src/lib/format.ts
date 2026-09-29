const rtf = new Intl.RelativeTimeFormat("ja", { numeric: "auto" });

export function relativeTime(iso?: string | null): string {
  if (!iso) return "";
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return iso;
  const diff = (t - Date.now()) / 1000;
  const abs = Math.abs(diff);
  if (abs < 60) return "たった今";
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
  if (abs < 86400 * 7) return rtf.format(Math.round(diff / 86400), "day");
  return shortDate(iso);
}

export function shortDate(iso?: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString("ja-JP", sameYear ? { month: "short", day: "numeric" } : { year: "numeric", month: "short", day: "numeric" });
}

export function longDate(iso?: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString("ja-JP", { year: "numeric", month: "long", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

export function todayLabel(day?: string): string {
  const d = day ? new Date(`${day}T00:00:00`) : new Date();
  return d.toLocaleDateString("ja-JP", { month: "long", day: "numeric", weekday: "long" });
}

export function compactNumber(n?: number | null): string {
  if (n == null) return "";
  return new Intl.NumberFormat("ja-JP", { notation: "compact", maximumFractionDigits: 1 }).format(n);
}

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

export type TextToken =
  | { type: "text"; value: string }
  | { type: "url"; value: string }
  | { type: "hashtag"; value: string }
  | { type: "mention"; value: string };

const TOKEN_RE = /(https?:\/\/[^\s<>"'）」]+)|([#＃][\p{L}\p{N}_]+)|(@[A-Za-z0-9_]{1,15})/gu;

/** ツイート本文を URL・ハッシュタグ・メンションに分ける */
export function tokenize(text: string): TextToken[] {
  const out: TextToken[] = [];
  let last = 0;
  for (const m of text.matchAll(TOKEN_RE)) {
    const i = m.index ?? 0;
    if (i > last) out.push({ type: "text", value: text.slice(last, i) });
    if (m[1]) out.push({ type: "url", value: m[1] });
    else if (m[2]) out.push({ type: "hashtag", value: m[2] });
    else if (m[3]) out.push({ type: "mention", value: m[3] });
    last = i + m[0].length;
  }
  if (last < text.length) out.push({ type: "text", value: text.slice(last) });
  return out;
}

export function displayUrl(url: string): string {
  const s = url.replace(/^https?:\/\/(www\.)?/, "");
  return s.length > 42 ? `${s.slice(0, 40)}…` : s;
}
