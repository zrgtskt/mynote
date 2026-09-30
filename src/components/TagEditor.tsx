import { useMemo, useRef, useState } from "react";
import { X } from "lucide-react";
import type { TagCount } from "../types";

interface Props {
  tags: string[];
  allTags: TagCount[];
  onChange: (tags: string[]) => void;
}

/** タグの追加・削除。既存タグを候補に出す */
export function TagEditor({ tags, allTags, onChange }: Props) {
  const [input, setInput] = useState("");
  const [focused, setFocused] = useState(false);
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const suggestions = useMemo(() => {
    const q = input.trim().replace(/^[#＃]/, "").toLowerCase();
    const own = new Set(tags.map((t) => t.toLowerCase()));
    return allTags
      .filter((t) => !own.has(t.name.toLowerCase()))
      .filter((t) => !q || t.name.toLowerCase().includes(q))
      .slice(0, 8);
  }, [input, tags, allTags]);

  const add = (raw: string) => {
    const t = raw.trim().replace(/^[#＃]/, "").trim();
    if (!t) return;
    if (!tags.some((x) => x.toLowerCase() === t.toLowerCase())) onChange([...tags, t]);
    setInput("");
    setCursor(0);
  };

  const showSuggest = focused && suggestions.length > 0;

  return (
    <div className="tag-editor" onClick={() => inputRef.current?.focus()}>
      {tags.map((t) => (
        <span key={t} className="chip chip-active">
          <span className="hash">#</span>
          {t}
          <button className="chip-x" aria-label={`${t} を外す`} onClick={() => onChange(tags.filter((x) => x !== t))}>
            <X />
          </button>
        </span>
      ))}
      <input
        ref={inputRef}
        value={input}
        placeholder={tags.length ? "タグを追加…" : "タグを追加（Enter で確定）"}
        onChange={(e) => {
          setInput(e.target.value);
          setCursor(0);
        }}
        onFocus={() => setFocused(true)}
        onBlur={() => setTimeout(() => setFocused(false), 120)}
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing) return;
          if (e.key === "ArrowDown" && showSuggest) {
            e.preventDefault();
            setCursor((c) => Math.min(c + 1, suggestions.length - 1));
          } else if (e.key === "ArrowUp" && showSuggest) {
            e.preventDefault();
            setCursor((c) => Math.max(c - 1, 0));
          } else if (e.key === "Enter" || e.key === ",") {
            e.preventDefault();
            if (showSuggest && input.trim() && suggestions[cursor] && suggestions[cursor].name.toLowerCase().startsWith(input.trim().toLowerCase())) {
              add(suggestions[cursor].name);
            } else if (input.trim()) {
              add(input);
            } else if (showSuggest && suggestions[cursor]) {
              add(suggestions[cursor].name);
            }
          } else if (e.key === "Backspace" && !input && tags.length) {
            onChange(tags.slice(0, -1));
          }
        }}
      />
      {showSuggest && (
        <div className="suggest">
          {suggestions.map((s, i) => (
            <button key={s.name} className={i === cursor ? "on" : ""} onMouseDown={(e) => e.preventDefault()} onClick={() => add(s.name)}>
              <span>
                <span className="hashtag">#</span>
                {s.name}
              </span>
              <span className="count">{s.count}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
