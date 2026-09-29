import { useEffect, useRef, useState } from "react";
import { CornerDownLeft, LoaderCircle } from "lucide-react";

interface Props {
  onClose: () => void;
  onSubmit: (urls: string[]) => Promise<void>;
}

/** 記事や X のポストの URL を追加する（改行区切りで複数可） */
export function AddUrlModal({ onClose, onSubmit }: Props) {
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    ref.current?.focus();
  }, []);

  const urls = value
    .split(/\s+/)
    .map((s) => s.trim())
    .filter((s) => /^https?:\/\//.test(s));

  const submit = async () => {
    if (!urls.length || busy) return;
    setBusy(true);
    try {
      await onSubmit(urls);
      onClose();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-wrap" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="modal" role="dialog" aria-label="URL を追加">
        <h2>URL を追加</h2>
        <p>Web 記事は本文を取り出して保存します。X のポストの URL も追加できます。複数の URL は改行で区切ってください。</p>
        <textarea
          ref={ref}
          className="field mono"
          style={{ height: 96, padding: "10px 14px", resize: "vertical" }}
          value={value}
          placeholder="https://example.com/article"
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) submit();
            if (e.key === "Enter" && !e.shiftKey && urls.length <= 1 && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
            if (e.key === "Escape" && !busy) onClose();
          }}
        />
        <div className="modal-actions">
          <span className="muted" style={{ marginRight: "auto", fontSize: 12, alignSelf: "center" }}>
            {urls.length > 1 ? `${urls.length} 件の URL` : ""}
          </span>
          <button className="btn btn-ghost" onClick={onClose} disabled={busy}>
            キャンセル
          </button>
          <button className="btn btn-primary" onClick={submit} disabled={!urls.length || busy}>
            {busy ? <LoaderCircle className="spin" /> : <CornerDownLeft />}
            保存する
          </button>
        </div>
      </div>
    </div>
  );
}
