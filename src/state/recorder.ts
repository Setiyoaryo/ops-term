import { create } from "zustand";

/**
 * A lightweight expect-script recorder. While armed, it watches what the user
 * sends to a session and turns typed lines into `sendline` steps. It is a
 * convenience, not a security boundary: a password typed at a prompt would be
 * captured too, so review the script before saving it as a runbook.
 */
interface RecorderState {
  recording: boolean;
  steps: string[];
  buffers: Record<string, string>;
  toggle: () => void;
  start: () => void;
  stop: () => void;
  reset: () => void;
  feed: (sessionId: string, data: string) => void;
  script: () => string;
}

export const useRecorder = create<RecorderState>((set, get) => ({
  recording: false,
  steps: [],
  buffers: {},
  toggle: () => set((s) => ({ recording: !s.recording })),
  start: () => set({ recording: true }),
  stop: () => set({ recording: false }),
  reset: () => set({ steps: [], buffers: {} }),
  feed: (sessionId, data) => {
    if (!get().recording) return;
    set((s) => {
      const buffers = { ...s.buffers };
      const steps = [...s.steps];
      let buf = buffers[sessionId] ?? "";
      for (const ch of data) {
        if (ch === "\r" || ch === "\n") {
          const line = buf.trim();
          if (line) steps.push(`sendline ${line}`);
          buf = "";
        } else if (ch === "\x7f" || ch === "\x08") {
          buf = buf.slice(0, -1);
        } else if (ch === "\x03") {
          buf = "";
        } else if (ch >= " ") {
          buf += ch;
        }
      }
      buffers[sessionId] = buf;
      return { buffers, steps };
    });
  },
  script: () => get().steps.join("\n"),
}));
