import { useEffect, useRef, useState } from "react";

import { getSearch } from "../../components/terminal/terminalRegistry";

interface Props {
  sessionId: string;
  onClose: () => void;
}

export function TerminalSearchBar({ sessionId, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const run = (forward: boolean) => {
    const search = getSearch(sessionId);
    if (!search || !query) return;
    const options = { caseSensitive, incremental: true };
    if (forward) search.findNext(query, options);
    else search.findPrevious(query, options);
  };

  const close = () => {
    getSearch(sessionId)?.clearDecorations();
    onClose();
  };

  return (
    <div
      data-testid="terminal-search"
      className="absolute right-3 top-3 z-20 flex items-center gap-2 rounded border border-[#2B323A] bg-[#181C20] px-2 py-1 text-xs text-[#E5E9EF] shadow-lg"
    >
      <input
        ref={inputRef}
        aria-label="Find in terminal"
        className="w-48 rounded border border-[#2B323A] bg-[#111418] px-2 py-1 outline-none focus:border-[#4F8EE8]"
        value={query}
        placeholder="Find..."
        onChange={(e) => {
          setQuery(e.target.value);
          const search = getSearch(sessionId);
          if (search && e.target.value) {
            search.findNext(e.target.value, { caseSensitive, incremental: true });
          } else {
            search?.clearDecorations();
          }
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            run(!e.shiftKey);
          } else if (e.key === "Escape") {
            e.preventDefault();
            close();
          }
        }}
      />
      <label className="flex items-center gap-1">
        <input
          type="checkbox"
          checked={caseSensitive}
          onChange={(e) => setCaseSensitive(e.target.checked)}
        />
        Aa
      </label>
      <button
        type="button"
        title="Previous (Shift+Enter)"
        className="rounded px-1 hover:bg-[#2B323A]"
        onClick={() => run(false)}
      >
        ↑
      </button>
      <button
        type="button"
        title="Next (Enter)"
        className="rounded px-1 hover:bg-[#2B323A]"
        onClick={() => run(true)}
      >
        ↓
      </button>
      <button
        type="button"
        title="Close (Esc)"
        className="rounded px-1 hover:bg-[#2B323A]"
        onClick={close}
      >
        ✕
      </button>
    </div>
  );
}
