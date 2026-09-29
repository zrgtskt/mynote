// Rust 側（src-tauri/src/models.rs）と対応する型

export interface Media {
  kind: string;
  url?: string | null;
  previewUrl?: string | null;
  videoUrl?: string | null;
  width?: number | null;
  height?: number | null;
  alt?: string | null;
  localPath?: string | null;
}

export interface LinkCard {
  url: string;
  title?: string | null;
  description?: string | null;
  image?: string | null;
}

export interface Item {
  id: number;
  kind: "tweet" | "article";
  url: string;
  externalId?: string | null;
  title?: string | null;
  authorName?: string | null;
  authorHandle?: string | null;
  authorAvatar?: string | null;
  authorAvatarLocal?: string | null;
  siteName?: string | null;
  text: string;
  excerpt?: string | null;
  summary?: string | null;
  imageUrl?: string | null;
  imageLocal?: string | null;
  media: Media[];
  link?: LinkCard | null;
  metrics?: Record<string, number> | null;
  publishedAt?: string | null;
  savedAt: string;
  isLiked: boolean;
  isBookmarked: boolean;
  isManual: boolean;
  aiTaggedAt?: string | null;
  note?: string | null;
  hasContent: boolean;
  tags: string[];
}

export interface ItemDetail extends Item {
  contentHtml?: string | null;
}

export type Filter = "all" | "liked" | "bookmarked" | "tweet" | "article" | "manual" | "untagged";
export type Sort = "saved_desc" | "saved_asc" | "published_desc" | "random";
export type TagMode = "and" | "or";

export interface ItemQuery {
  keyword?: string;
  tags?: string[];
  tagMode?: TagMode;
  filter?: Filter;
  sort?: Sort;
  limit?: number;
  offset?: number;
}

export interface ItemPage {
  items: Item[];
  total: number;
}

export interface TagCount {
  name: string;
  count: number;
}

export interface Stats {
  total: number;
  liked: number;
  bookmarked: number;
  tweets: number;
  articles: number;
  manual: number;
  untagged: number;
  tags: number;
}

export interface DailyPickup {
  day: string;
  items: Item[];
}

export interface Settings {
  xClientId: string;
  hasXClientSecret: boolean;
  xRedirectUri: string;
  xConnected: boolean;
  xUsername?: string | null;
  xName?: string | null;
  xAvatar?: string | null;
  syncLikes: boolean;
  syncBookmarks: boolean;
  syncMaxItems: number;
  autoSync: boolean;
  lastSyncAt?: string | null;
  lastSyncSummary?: string | null;
  hasAnthropicKey: boolean;
  anthropicKeyFromEnv: boolean;
  claudeModel: string;
  autoTag: boolean;
  pickupCount: number;
  downloadMedia: boolean;
  dataDir: string;
}

export type SettingsPatch = Partial<{
  xClientId: string;
  xClientSecret: string;
  syncLikes: boolean;
  syncBookmarks: boolean;
  syncMaxItems: number;
  autoSync: boolean;
  anthropicApiKey: string;
  claudeModel: string;
  autoTag: boolean;
  pickupCount: number;
  downloadMedia: boolean;
}>;

export interface Progress {
  task: "sync" | "tag" | "media" | "add" | string;
  message: string;
  current: number;
  total: number;
  done: boolean;
}

export interface SyncReport {
  likedFetched: number;
  likedNew: number;
  bookmarksFetched: number;
  bookmarksNew: number;
  tagged: number;
  mediaSaved: number;
  errors: string[];
}

export interface TagReport {
  tagged: number;
  failed: number;
  errors: string[];
}

export type View =
  | { name: "pickup" }
  | { name: "items" }
  | { name: "tags" }
  | { name: "settings" };
