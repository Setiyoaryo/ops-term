import { create } from "zustand";

export interface HighlightPreset {
  id: string;
  name: string;
  pattern: string;
  caseSensitive: boolean;
  regex: boolean;
  enabled: boolean;
  color: string;
}

const STORAGE_KEY = "opsterm.highlightPresets";

function load(): HighlightPreset[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults();
    return JSON.parse(raw) as HighlightPreset[];
  } catch {
    return defaults();
  }
}

function defaults(): HighlightPreset[] {
  return [
    { id: "errors", name: "Errors", pattern: "error|failed|down|invalid", regex: true, caseSensitive: false, enabled: true, color: "#E06C75" },
    { id: "routing", name: "Routing", pattern: "BGP|OSPF|ISIS|EVPN", regex: true, caseSensitive: false, enabled: true, color: "#4F8EE8" },
    { id: "ipv4", name: "IPv4", pattern: "\\b\\d{1,3}(\\.\\d{1,3}){3}\\b", regex: true, caseSensitive: false, enabled: false, color: "#E5C07B" },
  ];
}

function persist(presets: HighlightPreset[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(presets));
  } catch {
    /* storage unavailable */
  }
}

interface HighlightState {
  presets: HighlightPreset[];
  upsert: (preset: HighlightPreset) => void;
  remove: (id: string) => void;
  toggle: (id: string) => void;
}

export const useHighlights = create<HighlightState>((set) => ({
  presets: load(),
  upsert: (preset) =>
    set((s) => {
      const exists = s.presets.some((p) => p.id === preset.id);
      const presets = exists
        ? s.presets.map((p) => (p.id === preset.id ? preset : p))
        : [...s.presets, preset];
      persist(presets);
      return { presets };
    }),
  remove: (id) =>
    set((s) => {
      const presets = s.presets.filter((p) => p.id !== id);
      persist(presets);
      return { presets };
    }),
  toggle: (id) =>
    set((s) => {
      const presets = s.presets.map((p) =>
        p.id === id ? { ...p, enabled: !p.enabled } : p,
      );
      persist(presets);
      return { presets };
    }),
}));
