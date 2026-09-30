// 画像ファイル（ドロップ・選択・貼り付け）を取り込む
import { api, errorMessage } from "../api";
import type { Item } from "../types";

export const IMAGE_ACCEPT = "image/jpeg,image/png,image/gif,image/webp";
const SUPPORTED = new Set(IMAGE_ACCEPT.split(","));
const MAX_BYTES = 50 * 1024 * 1024;

export function isImageFile(f: File): boolean {
  return f.type.startsWith("image/") || /\.(jpe?g|png|gif|webp|heic|heif|avif|bmp|tiff?)$/i.test(f.name);
}

/** DataTransfer（ドロップ・貼り付け）から画像ファイルだけを取り出す */
export function imageFilesFrom(dt: DataTransfer | null): File[] {
  if (!dt) return [];
  return Array.from(dt.files).filter(isImageFile);
}

function toBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result);
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.onerror = () => reject(reader.error ?? new Error("ファイルを読めません"));
    reader.readAsDataURL(file);
  });
}

export interface ImportResult {
  items: Item[];
  errors: string[];
}

/** 画像を 1 枚ずつ取り込む（仕分けは取り込み後にバックエンドが裏で行う） */
export async function importImageFiles(files: File[], onProgress?: (done: number, total: number) => void): Promise<ImportResult> {
  const items: Item[] = [];
  const errors: string[] = [];
  let done = 0;
  for (const f of files) {
    const name = f.name || "貼り付けた画像";
    try {
      if (f.type && !SUPPORTED.has(f.type)) {
        throw new Error("JPEG・PNG・GIF・WebP のみ取り込めます（HEIC などは JPEG に書き出してください）");
      }
      if (f.size > MAX_BYTES) throw new Error("50 MB を超える画像は取り込めません");
      const data = await toBase64(f);
      const modified = f.lastModified ? new Date(f.lastModified).toISOString() : undefined;
      items.push(await api.importImage(name, data, modified));
    } catch (e) {
      errors.push(`${name}: ${errorMessage(e)}`);
    }
    onProgress?.(++done, files.length);
  }
  return { items, errors };
}
