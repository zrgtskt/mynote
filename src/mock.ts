// ブラウザだけで UI を確認するための仮バックエンド（`npm run dev` でブラウザを開いたときだけ使う）
import type { DailyPickup, Item, ItemPage, ItemQuery, Progress, Settings, Stats, TagCount } from "./types";

const svg = (body: string, w = 600, h = 400) =>
  `data:image/svg+xml;utf8,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${body}</svg>`,
  )}`;

const avatar = (letter: string, a: string, b: string) =>
  svg(
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs>
     <rect width="96" height="96" fill="url(#g)"/><text x="48" y="62" font-size="40" font-family="sans-serif" font-weight="700" fill="white" text-anchor="middle">${letter}</text>`,
    96,
    96,
  );

const cover = (a: string, b: string, label: string) =>
  svg(
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs>
     <rect width="600" height="340" fill="url(#g)"/>
     <circle cx="480" cy="70" r="120" fill="white" fill-opacity="0.08"/><circle cx="90" cy="300" r="90" fill="white" fill-opacity="0.06"/>
     <text x="40" y="300" font-size="34" font-family="sans-serif" font-weight="700" fill="white" fill-opacity="0.9">${label}</text>`,
    600,
    340,
  );

const now = Date.now();
const ago = (h: number) => new Date(now - h * 3600_000).toISOString();

let nextId = 1;
const base = (p: Partial<Item>): Item => ({
  id: nextId++,
  kind: "tweet",
  url: "https://x.com/",
  text: "",
  media: [],
  savedAt: ago(nextId * 5),
  isLiked: false,
  isBookmarked: false,
  isManual: false,
  hasContent: false,
  tags: [],
  ...p,
});

const items: Item[] = [
  base({
    authorName: "Rust 日本語ニュース",
    authorHandle: "rust_jp",
    authorAvatar: avatar("R", "#f97316", "#ef4444"),
    text: "Tauri 2 で作るデスクトップアプリ、バンドルサイズが 10MB を切るのは本当にすごい。フロントは React、バックエンドは Rust でそのまま SQLite を叩ける構成が気持ちいい 🦀\nhttps://v2.tauri.app/",
    isLiked: true,
    tags: ["Rust", "Tauri", "デスクトップアプリ"],
    summary: "Tauri 2 の軽さと React＋Rust 構成の良さを紹介",
    metrics: { like_count: 1280, retweet_count: 210 },
    publishedAt: ago(30),
    link: { url: "https://v2.tauri.app/", title: "Tauri 2.0", description: "Create small, fast, secure, cross-platform applications", image: cover("#1e3a8a", "#0ea5e9", "Tauri 2.0") },
    linked: {
      url: "https://v2.tauri.app/",
      title: "Tauri 2.0 | Tauri",
      siteName: "Tauri",
      excerpt: "Create small, fast, secure, cross-platform applications",
      imageUrl: cover("#1e3a8a", "#0ea5e9", "Tauri 2.0"),
      fetchedAt: ago(29),
      hasContent: true,
    },
  }),
  base({
    kind: "article",
    url: "https://example.com/sqlite-fts",
    title: "SQLite の FTS5 と trigram で日本語全文検索を作る",
    siteName: "Tech Notes",
    authorName: "山田花子",
    imageUrl: cover("#4c1d95", "#7c3aed", "SQLite FTS5"),
    text: "SQLite の FTS5 には trigram トークナイザーがあり、形態素解析なしでも日本語の部分一致検索ができる。この記事では…",
    excerpt: "形態素解析なしで日本語の部分一致検索を実現する方法を、実際のスキーマとクエリ例で解説します。",
    summary: "FTS5 の trigram で日本語全文検索を作る手順の解説",
    isManual: true,
    hasContent: true,
    tags: ["SQLite", "全文検索", "データベース"],
    publishedAt: ago(80),
  }),
  base({
    authorName: "料理研究家ゆき",
    authorHandle: "yuki_kitchen",
    authorAvatar: avatar("ゆ", "#ec4899", "#f59e0b"),
    text: "無水カレーのコツ：玉ねぎは飴色になるまで炒めなくていい。トマトと一緒に蓋をして弱火で 30 分、それだけで甘みが全然違う。スパイスは最後に。",
    media: [
      { kind: "photo", url: cover("#b45309", "#f59e0b", "無水カレー"), width: 600, height: 340 },
      { kind: "photo", url: cover("#7c2d12", "#ea580c", "スパイス"), width: 600, height: 340 },
    ],
    isBookmarked: true,
    tags: ["料理", "レシピ"],
    summary: "無水カレーを甘く仕上げる加熱とスパイスのコツ",
    metrics: { like_count: 5400, retweet_count: 890 },
    publishedAt: ago(50),
  }),
  base({
    authorName: "デザインのひきだし",
    authorHandle: "design_drawer",
    authorAvatar: avatar("D", "#06b6d4", "#6366f1"),
    text: "ダークテーマの配色で大事なのは「真っ黒を使わない」こと。#0B0D17 くらいの青みがかった黒に、彩度を落としたアクセントを 1 色。カードは背景より 4〜6% 明るくすると奥行きが出る。",
    isLiked: true,
    isBookmarked: true,
    tags: ["デザイン", "UI", "配色"],
    summary: "ダークテーマ配色の基本（真っ黒を避け、明度差で奥行きを出す）",
    metrics: { like_count: 3100, retweet_count: 640 },
    publishedAt: ago(120),
  }),
  base({
    kind: "article",
    url: "https://example.com/index-investing",
    title: "インデックス投資を 10 年続けてわかったこと",
    siteName: "マネーノート",
    imageUrl: cover("#065f46", "#10b981", "10 years"),
    text: "毎月一定額を積み立てるだけのシンプルな方法だが、続けるうえで大事なのは暴落時に売らないこと。",
    excerpt: "積立を 10 年続けた実体験から、暴落時の心構えと資産配分の見直し方をまとめました。",
    summary: "10 年間のインデックス投資の実体験と続けるコツ",
    isManual: true,
    hasContent: true,
    tags: ["投資", "お金"],
    publishedAt: ago(300),
  }),
  base({
    authorName: "ML エンジニアの日常",
    authorHandle: "ml_daily",
    authorAvatar: avatar("M", "#22c55e", "#0ea5e9"),
    text: "小さなモデルでも、評価セットを先に作ってからプロンプトを直すと改善の速度がまるで違う。「なんとなく良くなった」をやめて、数字で比べるのが一番の近道。",
    isLiked: true,
    tags: ["機械学習", "LLM", "評価"],
    summary: "評価セットを先に作ってからプロンプトを改善する重要性",
    metrics: { like_count: 890, retweet_count: 120 },
    publishedAt: ago(200),
  }),
  base({
    authorName: "旅と写真",
    authorHandle: "tabi_photo",
    authorAvatar: avatar("旅", "#8b5cf6", "#ec4899"),
    text: "秋の京都、朝 7 時の嵐山は人がほとんどいなくて最高だった。竹林の光がきれい。",
    media: [{ kind: "photo", url: cover("#14532d", "#65a30d", "嵐山 7:00"), width: 600, height: 340 }],
    isLiked: true,
    tags: ["旅行", "京都", "写真"],
    summary: "早朝の嵐山・竹林がすいていて美しかった話",
    metrics: { like_count: 2200, retweet_count: 150 },
    publishedAt: ago(400),
  }),
  base({
    authorName: "フロントエンド通信",
    authorHandle: "fe_tsushin",
    authorAvatar: avatar("F", "#0ea5e9", "#22d3ee"),
    text: "React 19 の use() と Suspense の組み合わせ、データ取得の書き方がかなりすっきりする。サンプルを置いておきます 👇 https://react.dev/blog",
    link: { url: "https://react.dev/blog", title: "React Blog" },
    isBookmarked: true,
    tags: ["React", "フロントエンド"],
    summary: "React 19 の use() と Suspense でデータ取得を簡潔に",
    publishedAt: ago(26),
  }),
  base({
    kind: "article",
    url: "https://example.com/sleep",
    title: "睡眠の質を上げる 7 つの習慣",
    siteName: "Health Lab",
    imageUrl: cover("#1e1b4b", "#4338ca", "Sleep"),
    text: "寝る 1 時間前に画面を見ない、朝に日光を浴びる、カフェインは 14 時まで…",
    excerpt: "研究でわかっている、睡眠の質を上げるための具体的な生活習慣を紹介します。",
    isManual: true,
    hasContent: true,
    tags: [],
    publishedAt: ago(500),
  }),
  base({
    authorName: "個人開発者のメモ",
    authorHandle: "indie_memo",
    authorAvatar: avatar("個", "#f43f5e", "#8b5cf6"),
    text: "個人開発で一番効いたのは「毎日 30 分だけ触る」ルール。やる気がある日は勝手に伸びるし、ない日も 30 分なら続く。",
    isLiked: true,
    tags: ["個人開発", "習慣"],
    summary: "毎日 30 分だけ触るルールで個人開発を継続するコツ",
    publishedAt: ago(700),
  }),
  base({
    authorName: "Rust 日本語ニュース",
    authorHandle: "rust_jp",
    authorAvatar: avatar("R", "#f97316", "#ef4444"),
    text: "rusqlite の bundled feature を使うと、ユーザーの環境に SQLite が入っていなくても動く。FTS5 も最初から有効。",
    isBookmarked: true,
    tags: ["Rust", "SQLite"],
    publishedAt: ago(900),
  }),
];

// 取り込んだ画像のサンプル
const screenshot = svg(
  `<rect width="720" height="1280" fill="#0f172a"/><rect x="0" y="0" width="720" height="120" fill="#1e293b"/>
   <text x="40" y="78" font-size="36" font-family="sans-serif" font-weight="700" fill="#e2e8f0">材料（2人分）</text>
   ${["玉ねぎ 1個", "トマト缶 1缶", "鶏もも肉 300g", "カレー粉 大さじ2", "塩 少々"].map((t, i) => `<text x="60" y="${220 + i * 90}" font-size="40" font-family="sans-serif" fill="#cbd5e1">・${t}</text>`).join("")}
   <rect x="40" y="760" width="640" height="360" rx="24" fill="#f59e0b" fill-opacity="0.25"/>
   <text x="80" y="840" font-size="34" font-family="sans-serif" fill="#fde68a">弱火で 30 分煮込む</text>`,
  720,
  1280,
);
const photo = svg(
  `<defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fb923c"/><stop offset="0.55" stop-color="#f472b6"/><stop offset="1" stop-color="#1e1b4b"/></linearGradient></defs>
   <rect width="1200" height="800" fill="url(#s)"/><circle cx="600" cy="470" r="110" fill="#fde68a" fill-opacity="0.9"/>
   <rect y="560" width="1200" height="240" fill="#0f172a" fill-opacity="0.85"/>`,
  1200,
  800,
);
const diagram = svg(
  `<rect width="1000" height="640" fill="#f8fafc"/>
   ${[["フロント", 80], ["Rust", 400], ["SQLite", 720]].map(([t, x]) => `<rect x="${x}" y="250" width="200" height="120" rx="18" fill="#6366f1"/><text x="${Number(x) + 100}" y="320" font-size="30" font-family="sans-serif" fill="white" text-anchor="middle">${t}</text>`).join("")}
   <path d="M280 310 H400 M600 310 H720" stroke="#334155" stroke-width="6"/>`,
  1000,
  640,
);
items.push(
  base({
    kind: "image",
    url: "",
    title: "スクリーンショット 2026-09-27 12.04.33",
    text: "材料（2人分）\n・玉ねぎ 1個\n・トマト缶 1缶\n・鶏もも肉 300g\n・カレー粉 大さじ2\n・塩 少々\n弱火で 30 分煮込む",
    summary: "チキンカレーの材料と煮込み時間をまとめたレシピ画面",
    file: { path: screenshot, width: 720, height: 1280, mime: "image/png", size: 348_000 },
    isManual: true,
    aiTaggedAt: ago(40),
    tags: ["スクリーンショット", "料理", "レシピ"],
    publishedAt: ago(48),
  }),
  base({
    kind: "image",
    url: "",
    title: "IMG_2041",
    summary: "海に沈む夕日とピンク色に染まった空",
    file: { path: photo, width: 1200, height: 800, mime: "image/jpeg", size: 2_400_000 },
    isManual: true,
    aiTaggedAt: ago(60),
    tags: ["写真", "旅行"],
    publishedAt: ago(200),
  }),
  base({
    kind: "image",
    url: "https://example.com/arch.png",
    siteName: "example.com",
    title: "arch",
    text: "フロント Rust SQLite",
    summary: "フロントエンド・Rust・SQLite の 3 層構成を示した図",
    file: { path: diagram, width: 1000, height: 640, mime: "image/png", size: 52_000 },
    isManual: true,
    aiTaggedAt: ago(80),
    tags: ["図解", "Rust", "SQLite"],
    publishedAt: ago(90),
  }),
);

// 無限スクロールの確認用に件数を増やす
const topics = ["読書メモ", "プログラミング", "写真", "料理", "旅行", "投資", "デザイン", "音楽"];
for (let i = 0; i < 70; i++) {
  const topic = topics[i % topics.length];
  items.push(
    base({
      authorName: `サンプル ${i + 1}`,
      authorHandle: `sample_${i + 1}`,
      text: `${topic}についてのサンプルのポスト（${i + 1} 件目）。保存したものが増えても、スクロールで続きを読み込めます。`,
      isLiked: i % 2 === 0,
      isBookmarked: i % 3 === 0,
      tags: [topic],
      publishedAt: ago(1000 + i * 12),
      savedAt: ago(1000 + i * 12),
    }),
  );
}

let pickDay = "";
let picks: number[] = [];

const settings: Settings = {
  xClientId: "",
  hasXClientSecret: false,
  xRedirectUri: "http://127.0.0.1:8723/callback",
  xConnected: true,
  xUsername: "your_handle",
  xName: "あなた",
  xAvatar: avatar("あ", "#7c6cff", "#2fd4ff"),
  syncLikes: true,
  syncBookmarks: true,
  syncMaxItems: 1000,
  autoSync: true,
  lastSyncAt: ago(3),
  lastSyncSummary: "いいね 20 件（新規 3）・ブックマーク 20 件（新規 1）を取得",
  hasAnthropicKey: true,
  anthropicKeyFromEnv: false,
  claudeModel: "claude-opus-5-5",
  autoTag: true,
  pickupCount: 5,
  downloadMedia: true,
  fetchLinks: true,
  dataDir: "~/.local/share/dev.mynote.desktop",
};

function tagCounts(): TagCount[] {
  const m = new Map<string, number>();
  for (const it of items) for (const t of it.tags) m.set(t, (m.get(t) ?? 0) + 1);
  return [...m.entries()].map(([name, count]) => ({ name, count })).sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
}

function matches(it: Item, q: ItemQuery): boolean {
  switch (q.filter) {
    case "liked": if (!it.isLiked) return false; break;
    case "bookmarked": if (!it.isBookmarked) return false; break;
    case "tweet": if (it.kind !== "tweet") return false; break;
    case "article": if (it.kind !== "article") return false; break;
    case "image": if (it.kind !== "image") return false; break;
    case "manual": if (!it.isManual) return false; break;
    case "untagged": if (it.tags.length) return false; break;
  }
  const hay = [it.title, it.text, it.authorName, it.authorHandle, it.siteName, it.summary, it.note].join(" ").toLowerCase();
  for (const raw of (q.keyword ?? "").split(/[\s　]+/).filter(Boolean)) {
    const neg = raw.startsWith("-") && raw.length > 1;
    const term = (neg ? raw.slice(1) : raw).toLowerCase();
    if (hay.includes(term) === neg) return false;
  }
  const tags = (q.tags ?? []).map((t) => t.toLowerCase());
  if (tags.length) {
    const own = it.tags.map((t) => t.toLowerCase());
    const ok = q.tagMode === "or" ? tags.some((t) => own.includes(t)) : tags.every((t) => own.includes(t));
    if (!ok) return false;
  }
  return true;
}

const listeners = new Map<string, Set<(p: unknown) => void>>();
const emit = (name: string, payload: unknown) => listeners.get(name)?.forEach((h) => h(payload));
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const handlers: Record<string, (args: Record<string, any>) => unknown> = {
  get_settings: () => ({ ...settings }),
  save_settings: ({ patch }) => {
    Object.assign(settings, patch);
    if ("anthropicApiKey" in patch) settings.hasAnthropicKey = !!patch.anthropicApiKey;
    return { ...settings };
  },
  x_connect: () => ({ ...settings, xConnected: true }),
  x_disconnect: () => Object.assign(settings, { xConnected: false, xUsername: null, xName: null }),
  sync_now: async () => {
    for (let i = 0; i <= 3; i++) {
      emit("progress", { task: "sync", message: `いいねを取得中…（${i * 20} 件）`, current: i, total: 3, done: false } satisfies Progress);
      await sleep(350);
    }
    settings.lastSyncAt = new Date().toISOString();
    emit("progress", { task: "sync", message: settings.lastSyncSummary!, current: 1, total: 1, done: true });
    return { likedFetched: 20, likedNew: 3, bookmarksFetched: 20, bookmarksNew: 1, tagged: 0, mediaSaved: 0, errors: [] };
  },
  list_items: ({ query }): ItemPage => {
    const q = query as ItemQuery;
    let list = items.filter((it) => matches(it, q));
    if (q.sort === "saved_asc") list = [...list].reverse();
    if (q.sort === "published_desc") list = [...list].sort((a, b) => (b.publishedAt ?? "").localeCompare(a.publishedAt ?? ""));
    if (q.sort === "random") list = [...list].sort(() => Math.random() - 0.5);
    const off = q.offset ?? 0;
    return { items: list.slice(off, off + (q.limit ?? 60)), total: list.length };
  },
  get_item: ({ id }) => {
    const it = items.find((i) => i.id === id);
    if (!it) return null;
    return {
      ...it,
      contentHtml: it.kind === "article"
        ? `<p>${it.excerpt ?? ""}</p><h2>はじめに</h2><p>${it.text}</p><blockquote>引用のサンプルです。</blockquote><p>詳しくは<a href="https://example.com">元の記事</a>を参照してください。</p><pre><code>CREATE VIRTUAL TABLE items_fts USING fts5(title, body, tokenize = 'trigram');</code></pre>`
        : null,
      linkedHtml: it.linked?.hasContent
        ? `<h2>Tauri 2.0 とは</h2><p>Tauri は、Web フロントエンドと Rust で小さく速く安全なデスクトップ・モバイルアプリを作るためのフレームワークです。</p><p>この本文は、ポストの紹介先ページとしてローカルに保存されたものです。元のページが消えても読み返せます。</p><blockquote>Build smaller, faster, and more secure applications.</blockquote>`
        : null,
      linkedText: it.linked?.hasContent ? "Tauri は…" : null,
    };
  },
  get_stats: (): Stats => ({
    total: items.length,
    liked: items.filter((i) => i.isLiked).length,
    bookmarked: items.filter((i) => i.isBookmarked).length,
    tweets: items.filter((i) => i.kind === "tweet").length,
    articles: items.filter((i) => i.kind === "article").length,
    manual: items.filter((i) => i.isManual).length,
    images: items.filter((i) => i.kind === "image").length,
    untagged: items.filter((i) => !i.tags.length).length,
    tags: tagCounts().length,
  }),
  list_tags: () => tagCounts(),
  set_item_tags: ({ id, tags }) => {
    const it = items.find((i) => i.id === id)!;
    it.tags = [...new Set((tags as string[]).map((t) => t.replace(/^[#＃]/, "").trim()).filter(Boolean))];
    return it.tags;
  },
  rename_tag: ({ from, to }) => {
    for (const it of items) it.tags = [...new Set(it.tags.map((t) => (t === from ? to : t)))];
  },
  delete_tag: ({ name }) => {
    for (const it of items) it.tags = it.tags.filter((t) => t !== name);
  },
  set_note: ({ id, note }) => {
    items.find((i) => i.id === id)!.note = note;
  },
  delete_item: ({ id }) => {
    items.splice(items.findIndex((i) => i.id === id), 1);
  },
  add_url: async ({ url }) => {
    emit("progress", { task: "add", message: "取得中…", current: 0, total: 1, done: false });
    await sleep(600);
    const it = base({ kind: "article", url, title: "追加した記事", siteName: new URL(url).hostname, text: "本文", excerpt: "追加した記事の概要です。", isManual: true, savedAt: new Date().toISOString() });
    items.unshift(it);
    emit("progress", { task: "add", message: "追加しました", current: 1, total: 1, done: true });
    return it;
  },
  import_image: async ({ name, data, modifiedAt }) => {
    await sleep(300);
    const mime = data.startsWith("/9j/") ? "image/jpeg" : "image/png";
    const it = base({
      kind: "image",
      url: "",
      title: String(name).replace(/\.[^.]+$/, ""),
      file: { path: `data:${mime};base64,${data}`, mime, size: Math.round((data.length * 3) / 4) },
      isManual: true,
      publishedAt: modifiedAt ?? null,
      savedAt: new Date().toISOString(),
    });
    items.unshift(it);
    // Claude の仕分けを模擬する
    setTimeout(() => {
      it.tags = ["スクリーンショット"];
      it.summary = "取り込んだ画像（仮データでの仕分け結果）";
      it.aiTaggedAt = new Date().toISOString();
      emit("items-changed", null);
    }, 1500);
    return it;
  },
  open_item_file: () => undefined,
  tag_pending: async () => ({ tagged: 1, failed: 0, errors: [] }),
  count_pending_tags: () => items.filter((i) => !i.aiTaggedAt && !i.tags.length).length,
  retag_item: async ({ id }) => {
    await sleep(800);
    const it = items.find((i) => i.id === id)!;
    it.tags = [...new Set([...it.tags, "AI"])];
    return handlers.get_item({ id });
  },
  daily_pickup: ({ reshuffle }): DailyPickup => {
    const day = new Date().toISOString().slice(0, 10);
    if (reshuffle || pickDay !== day || !picks.length) {
      pickDay = day;
      picks = [...items].sort(() => Math.random() - 0.5).slice(0, settings.pickupCount).map((i) => i.id);
    }
    return { day, items: picks.map((id) => items.find((i) => i.id === id)!).filter(Boolean) };
  },
  refetch_linked: async ({ id }) => {
    await sleep(700);
    const it = items.find((i) => i.id === id)!;
    it.linked = { url: it.link?.url ?? it.url, title: it.link?.title ?? "紹介先", siteName: "Example", fetchedAt: new Date().toISOString(), hasContent: true };
    return handlers.get_item({ id });
  },
  fetch_linked_pending: async () => 0,
  count_pending_links: () => items.filter((i) => i.link && !i.linked?.hasContent).length,
  download_media: () => 0,
  open_data_dir: () => undefined,
  open_external: ({ url }) => {
    window.open(url, "_blank", "noopener");
  },
};

export const mockBackend = {
  async invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
    await sleep(60);
    const h = handlers[cmd];
    if (!h) throw new Error(`mock: 未対応のコマンド ${cmd}`);
    return structuredClone(await h(args)) as T;
  },
  async listen<T>(event: string, handler: (payload: T) => void) {
    const set = listeners.get(event) ?? new Set();
    set.add(handler as (p: unknown) => void);
    listeners.set(event, set);
    return () => set.delete(handler as (p: unknown) => void);
  },
  fileSrc: (p: string) => p,
};
