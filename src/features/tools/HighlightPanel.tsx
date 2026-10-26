import { useState } from "react";

import { useHighlights, type HighlightPreset } from "../../state/highlight";

export function HighlightPanel() {
  const presets = useHighlights((s) => s.presets);
  const upsert = useHighlights((s) => s.upsert);
  const remove = useHighlights((s) => s.remove);
  const toggle = useHighlights((s) => s.toggle);

  const [draft, setDraft] = useState<HighlightPreset>({
    id: "",
    name: "",
    pattern: "",
    caseSensitive: false,
    regex: true,
    enabled: true,
    color: "#E5C07B",
  });

  const field =
    "rounded border border-[#2B323A] bg-[#111418] px-2 py-0.5 text-xs text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  const save = () => {
    if (!draft.name.trim() || !draft.pattern.trim()) return;
    upsert({
      ...draft,
      id: draft.id || `${Date.now()}`,
      name: draft.name.trim(),
      pattern: draft.pattern.trim(),
    });
    setDraft({ id: "", name: "", pattern: "", caseSensitive: false, regex: true, enabled: true, color: "#E5C07B" });
  };

  return (
    <div className="flex flex-col gap-2 text-xs">
      <p className="text-[#99A3AE]">
        Reusable highlight presets. Enable one and use it from the terminal
        search bar (Ctrl+F) to find matches without retyping patterns.
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <input
          className={field}
          placeholder="name"
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
        />
        <input
          className={`${field} flex-1`}
          placeholder="pattern"
          value={draft.pattern}
          onChange={(e) => setDraft({ ...draft, pattern: e.target.value })}
        />
        <input
          type="color"
          value={draft.color}
          onChange={(e) => setDraft({ ...draft, color: e.target.value })}
        />
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            checked={draft.regex}
            onChange={(e) => setDraft({ ...draft, regex: e.target.checked })}
          />
          regex
        </label>
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            checked={draft.caseSensitive}
            onChange={(e) => setDraft({ ...draft, caseSensitive: e.target.checked })}
          />
          Aa
        </label>
        <button
          type="button"
          onClick={save}
          className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8]"
        >
          {draft.id ? "Update" : "Add"}
        </button>
      </div>
      <div className="flex flex-col gap-1">
        {presets.map((p) => (
          <div
            key={p.id}
            className="flex items-center gap-2 rounded border border-[#2B323A] px-2 py-1"
          >
            <input type="checkbox" checked={p.enabled} onChange={() => toggle(p.id)} />
            <span style={{ color: p.color }}>●</span>
            <span className="w-24 truncate">{p.name}</span>
            <code className="flex-1 truncate text-[#99A3AE]">{p.pattern}</code>
            <button
              type="button"
              onClick={() => setDraft(p)}
              className="rounded border border-[#2B323A] px-1.5 hover:bg-[#1D2228]"
            >
              Edit
            </button>
            <button
              type="button"
              onClick={() => remove(p.id)}
              className="rounded border border-[#2B323A] px-1.5 hover:bg-[#1D2228] hover:text-red-400"
            >
              Delete
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
