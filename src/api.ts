import type {
  DailyPickup,
  Item,
  ItemDetail,
  ItemPage,
  ItemQuery,
  Progress,
  Settings,
  SettingsPatch,
  Stats,
  SyncReport,
  TagCount,
  TagReport,
} from "./types";

type Unlisten = () => void;

interface Backend {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<Unlisten>;
  fileSrc(path: string): string;
}

export const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let backendPromise: Promise<Backend> | null = null;

function backend(): Promise<Backend> {
  if (!backendPromise) {
    backendPromise = (async () => {
      if (isTauri()) {
        const core = await import("@tauri-apps/api/core");
        const event = await import("@tauri-apps/api/event");
        return {
          invoke: (cmd, args) => core.invoke(cmd, args),
          listen: (name, handler) => event.listen(name, (e) => handler(e.payload as never)),
          fileSrc: (path) => core.convertFileSrc(path),
        } satisfies Backend;
      }
      if (import.meta.env.DEV) {
        // ブラウザだけで画面を確認するための仮データ（開発時のみ）
        const { mockBackend } = await import("./mock");
        return mockBackend;
      }
      throw new Error("mynote はデスクトップアプリとして起動してください（npm run app）");
    })();
  }
  return backendPromise;
}

let fileSrcFn: ((p: string) => string) | null = null;
backend().then((b) => (fileSrcFn = b.fileSrc)).catch(() => undefined);

const call = async <T>(cmd: string, args?: Record<string, unknown>) => (await backend()).invoke<T>(cmd, args);

export const api = {
  getSettings: () => call<Settings>("get_settings"),
  saveSettings: (patch: SettingsPatch) => call<Settings>("save_settings", { patch }),
  xConnect: () => call<Settings>("x_connect"),
  xDisconnect: () => call<Settings>("x_disconnect"),
  syncNow: (full = false) => call<SyncReport>("sync_now", { full }),
  listItems: (query: ItemQuery) => call<ItemPage>("list_items", { query }),
  getItem: (id: number) => call<ItemDetail | null>("get_item", { id }),
  getStats: () => call<Stats>("get_stats"),
  listTags: () => call<TagCount[]>("list_tags"),
  setItemTags: (id: number, tags: string[]) => call<string[]>("set_item_tags", { id, tags }),
  renameTag: (from: string, to: string) => call<void>("rename_tag", { from, to }),
  deleteTag: (name: string) => call<void>("delete_tag", { name }),
  setNote: (id: number, note: string) => call<void>("set_note", { id, note }),
  deleteItem: (id: number) => call<void>("delete_item", { id }),
  addUrl: (url: string) => call<Item>("add_url", { url }),
  importImage: (name: string, data: string, modifiedAt?: string) => call<Item>("import_image", { name, data, modifiedAt }),
  openItemFile: (id: number) => call<void>("open_item_file", { id }),
  tagPending: (limit?: number) => call<TagReport>("tag_pending", { limit }),
  countPendingTags: () => call<number>("count_pending_tags"),
  retagItem: (id: number) => call<ItemDetail>("retag_item", { id }),
  refetchLinked: (id: number) => call<ItemDetail>("refetch_linked", { id }),
  fetchLinkedPending: () => call<number>("fetch_linked_pending"),
  countPendingLinks: () => call<number>("count_pending_links"),
  dailyPickup: (reshuffle = false) => call<DailyPickup>("daily_pickup", { reshuffle }),
  downloadMedia: () => call<number>("download_media"),
  openDataDir: () => call<void>("open_data_dir"),
  openExternal: (url: string) => call<void>("open_external", { url }),
};

export async function onProgress(handler: (p: Progress) => void): Promise<Unlisten> {
  return (await backend()).listen<Progress>("progress", handler);
}

export async function onItemsChanged(handler: () => void): Promise<Unlisten> {
  return (await backend()).listen<null>("items-changed", () => handler());
}

/** ローカルに保存済みの画像があればそれを、なければ元の URL を使う */
export function mediaSrc(local?: string | null, remote?: string | null): string | undefined {
  if (local && fileSrcFn) return fileSrcFn(local);
  return remote ?? undefined;
}

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
