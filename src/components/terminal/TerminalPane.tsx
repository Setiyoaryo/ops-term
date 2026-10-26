import { useCallback, useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { SerializeAddon } from "@xterm/addon-serialize";

import { connectionResize, connectionWrite } from "../../lib/ipc";
import { useHighlights } from "../../state/highlight";
import { fallbackSettings, useLibrary } from "../../state/library";
import { useRecorder } from "../../state/recorder";
import { terminalTheme } from "../../theme";
import {
  registerSearch,
  registerTerminal,
  saveState,
  takeState,
  unregisterTerminal,
} from "./terminalRegistry";

interface Props {
  sessionId: string;
  active: boolean;
  /** What the Backspace key sends: `del` (0x7f) or `bs` (0x08, Huawei/Cisco). */
  backspace?: "del" | "bs";
}

/** Multi-line or control-character paste asks for confirmation (PRD FR-12). */
function needsReview(text: string): boolean {
  return /[\r\n]/.test(text) && text.split(/\r?\n/).length > 1;
}

/**
 * Normalize line endings to a single CR. Network CLIs (Huawei/Cisco) execute on
 * CR; a bare LF is silently ignored and merges the next line into the current
 * command (the classic "10GE1/0/4110GE1/0/41" bug).
 */
function toCr(text: string): string {
  return text.replace(/\r\n|\r|\n/g, "\r");
}

export function TerminalPane({ sessionId, active, backspace = "del" }: Props) {
  const backspaceRef = useRef(backspace);
  backspaceRef.current = backspace;
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const actionsRef = useRef<{
    copy: () => void;
    paste: (text?: string) => void;
  } | null>(null);
  const decorationsRef = useRef<{ dispose: () => void }[]>([]);
  const highlightRef = useRef<() => void>(() => {});
  const settings = useLibrary((s) => s.settings);

  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  // Default: pace multi-line pastes. Slow network CLIs otherwise drop/merge
  // characters when a whole block arrives at once (the "10GE1/0/4210GE1/0/42"
  // symptom). Users can turn it off in the review dialog.
  const [paced, setPaced] = useState(true);

  // Send each line with a small delay, for slow devices that drop fast input.
  const sendPaced = (text: string) => {
    text.split(/\r?\n/).forEach((line, i) => {
      window.setTimeout(() => {
        void connectionWrite(sessionId, `${line}\r`).catch(() => {});
      }, i * 60);
    });
  };

  const copy = useCallback(() => {
    const term = termRef.current;
    const selection = term?.getSelection();
    if (selection) void navigator.clipboard.writeText(selection);
  }, []);

  // Send bytes straight to the session (CR-normalized), never through
  // xterm's paste path — that is what guarantees separate lines on Huawei/Cisco.
  const paste = useCallback(
    (text?: string) => {
      const deliver = (value: string) => {
        if (!value) return;
        if (needsReview(value)) setPendingPaste(value);
        else void connectionWrite(sessionId, toCr(value)).catch(() => {});
      };
      if (text !== undefined) {
        deliver(text);
        return;
      }
      void navigator.clipboard
        .readText()
        .then(deliver)
        .catch(() => {
          /* clipboard unavailable */
        });
    },
    [sessionId],
  );

  // Keep a stable handle for the one-time key/event handlers created below.
  actionsRef.current = { copy, paste };

  const options = settings?.terminal ?? fallbackSettings().terminal;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const opts = useLibrary.getState().settings?.terminal ?? fallbackSettings().terminal;
    const term = new Terminal({
      fontFamily: opts.fontFamily,
      fontSize: opts.fontSize,
      scrollback: opts.scrollback,
      cursorBlink: opts.cursorBlink,
      cursorStyle: opts.cursorStyle as "block" | "underline" | "bar",
      allowProposedApi: true,
      theme: terminalTheme(opts.colorScheme),
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new WebLinksAddon());
    const unicode = new Unicode11Addon();
    term.loadAddon(unicode);
    term.unicode.activeVersion = "11";
    const serialize = new SerializeAddon();
    term.loadAddon(serialize);

    term.open(container);
    try {
      fit.fit();
    } catch {
      /* container may not be measurable yet */
    }

    termRef.current = term;
    fitRef.current = fit;
    // Restore a hibernated snapshot before any buffered output is flushed.
    const snapshot = takeState(sessionId);
    if (snapshot) term.write(snapshot);
    registerTerminal(sessionId, term);
    registerSearch(sessionId, search);

    const runCopy = () => actionsRef.current?.copy();
    const runPaste = (text?: string) => actionsRef.current?.paste(text);

    // Multi-colour keyword highlighting (FR-08 P1): register a decoration per
    // match with the rule's own colour. Bounded to the most recent lines.
    const clearDecorations = () => {
      for (const d of decorationsRef.current) {
        try {
          d.dispose();
        } catch {
          /* ignore */
        }
      }
      decorationsRef.current = [];
    };
    const applyHighlights = () => {
      clearDecorations();
      const enabled = useHighlights
        .getState()
        .presets.filter((p) => p.enabled && p.pattern.trim().length > 0);
      if (enabled.length === 0) return;
      const buffer = term.buffer.active;
      const cursorAbs = buffer.baseY + buffer.cursorY;
      const start = Math.max(0, buffer.baseY - 2000);
      const end = buffer.baseY + term.rows + 1;
      for (const rule of enabled) {
        let re: RegExp;
        try {
          const source = rule.regex
            ? rule.pattern
            : rule.pattern.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
          re = new RegExp(source, rule.caseSensitive ? "g" : "gi");
        } catch {
          continue;
        }
        for (let y = start; y < end; y += 1) {
          const line = buffer.getLine(y);
          if (!line) continue;
          const text = line.translateToString(true);
          if (!text) continue;
          re.lastIndex = 0;
          let match: RegExpExecArray | null;
          while ((match = re.exec(text)) !== null) {
            if (match[0].length === 0) {
              re.lastIndex += 1;
              continue;
            }
            try {
              const marker = term.registerMarker(y - cursorAbs);
              if (!marker) continue;
              const dec = term.registerDecoration({
                marker,
                x: match.index,
                width: match[0].length,
                backgroundColor: rule.color,
                layer: "bottom",
              });
              if (dec) decorationsRef.current.push(dec);
              else marker.dispose();
            } catch {
              return; // proposed decoration API unavailable
            }
          }
        }
      }
    };
    highlightRef.current = applyHighlights;
    let highlightTimer: ReturnType<typeof setTimeout> | null = null;
    const scheduleHighlight = () => {
      if (highlightTimer) clearTimeout(highlightTimer);
      highlightTimer = setTimeout(applyHighlights, 250);
    };
    const writeSub = term.onWriteParsed(() => scheduleHighlight());
    scheduleHighlight();

    // Bell → OS notification when the window is not focused.
    const bellSub = term.onBell(() => {
      try {
        if (!document.hidden) return;
        if (Notification.permission === "granted") {
          new Notification("OpsTerm", { body: "Terminal bell" });
        } else if (Notification.permission !== "denied") {
          void Notification.requestPermission();
        }
      } catch {
        /* notifications unavailable */
      }
    });

    // Optional copy-on-select.
    const selSub = term.onSelectionChange(() => {
      if (!useLibrary.getState().settings?.terminal.copyOnSelect) return;
      const selection = term.getSelection();
      if (selection) void navigator.clipboard.writeText(selection);
    });

    const sendResize = () => {
      try {
        fit.fit();
      } catch {
        return;
      }
      if (term.cols > 0 && term.rows > 0) {
        void connectionResize(sessionId, term.cols, term.rows).catch(() => {});
      }
    };
    requestAnimationFrame(sendResize);

    const onData = term.onData((data) => {
      // Safety net for any paste path (xterm, WebView native, IME): network
      // CLIs execute on CR, so a bare LF is normalized here too. Normal typing
      // and control keys are unaffected (Enter already sends CR).
      const normalized = data.replace(/\r\n|\n/g, "\r");
      useRecorder.getState().feed(sessionId, normalized);
      void connectionWrite(sessionId, normalized).catch(() => {});
    });

    // Terminal clipboard shortcuts. Ctrl+C / Ctrl+V stay control characters for
    // the remote shell; copy/paste use Ctrl+Shift+C/V, Ctrl+Insert/Shift+Insert.
    term.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;
      const mod = event.ctrlKey || event.metaKey;
      // Copy: Ctrl+Shift+C or Ctrl+Insert, and Ctrl+C when text is selected
      // (otherwise Ctrl+C still sends ETX to the remote).
      if (mod && event.shiftKey && event.code === "KeyC") {
        event.preventDefault();
        runCopy();
        return false;
      }
      if (event.ctrlKey && event.code === "Insert") {
        event.preventDefault();
        runCopy();
        return false;
      }
      if (event.ctrlKey && !event.shiftKey && event.code === "KeyC") {
        if (term.hasSelection()) {
          event.preventDefault();
          runCopy();
          return false;
        }
        return true;
      }
      // Paste: Ctrl+Shift+V, Shift+Insert, or plain Ctrl+V. preventDefault
      // stops the WebView's native paste, so it is sent exactly once.
      if (mod && event.shiftKey && event.code === "KeyV") {
        event.preventDefault();
        runPaste();
        return false;
      }
      if (event.shiftKey && event.code === "Insert") {
        event.preventDefault();
        runPaste();
        return false;
      }
      if (mod && !event.shiftKey && event.code === "KeyV") {
        event.preventDefault();
        runPaste();
        return false;
      }
      // Normalize Backspace for legacy network CLIs (Huawei/Cisco expect 0x08).
      if (
        event.code === "Backspace" &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.metaKey
      ) {
        event.preventDefault();
        void connectionWrite(
          sessionId,
          backspaceRef.current === "bs" ? "\x08" : "\x7f",
        ).catch(() => {});
        return false;
      }
      return true;
    });

    // Swallow the WebView's native paste events entirely: paste is handled by a
    // single explicit path (key handler / context menu) so it can never be sent
    // twice or with un-normalized newlines.
    const onPasteCapture = (event: ClipboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
    };
    container.addEventListener("paste", onPasteCapture, true);

    const observer = new ResizeObserver(sendResize);
    observer.observe(container);

    return () => {
      container.removeEventListener("paste", onPasteCapture, true);
      observer.disconnect();
      onData.dispose();
      writeSub.dispose();
      selSub.dispose();
      bellSub.dispose();
      clearDecorations();
      if (highlightTimer) clearTimeout(highlightTimer);

      // Hibernate: keep only a small snapshot so idle tabs cost almost nothing.
      try {
        saveState(sessionId, serialize.serialize({ scrollback: 1000, excludeAltBuffer: true, excludeModes: true }));
      } catch {
        /* ignore serialize errors */
      }
      unregisterTerminal(sessionId);
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [sessionId]);

  // Re-apply highlights when the preset set changes.
  const presets = useHighlights((s) => s.presets);
  useEffect(() => {
    highlightRef.current?.();
  }, [presets, sessionId]);

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.fontFamily = options.fontFamily;
    term.options.fontSize = options.fontSize;
    term.options.scrollback = options.scrollback;
    term.options.cursorBlink = options.cursorBlink;
    term.options.cursorStyle = options.cursorStyle as "block" | "underline" | "bar";
    term.options.theme = terminalTheme(options.colorScheme);
    if (active) {
      requestAnimationFrame(() => {
        try {
          fitRef.current?.fit();
          void connectionResize(sessionId, term.cols, term.rows).catch(() => {});
        } catch {
          /* ignore */
        }
      });
    }
  }, [
    active,
    sessionId,
    options.fontFamily,
    options.fontSize,
    options.scrollback,
    options.cursorBlink,
    options.cursorStyle,
    options.colorScheme,
  ]);

  useEffect(() => {
    if (!active) return;
    const term = termRef.current;
    if (!term) return;
    requestAnimationFrame(() => {
      try {
        fitRef.current?.fit();
        void connectionResize(sessionId, term.cols, term.rows).catch(() => {});
      } catch {
        /* ignore */
      }
      term.focus();
    });
  }, [active, sessionId]);

  const closeMenu = () => setMenu(null);

  return (
    <>
      <div
        ref={containerRef}
        className="h-full w-full"
        onMouseDown={() => {
          if (menu) closeMenu();
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY });
        }}
      />

      {menu && (
        <>
          <div className="fixed inset-0 z-40" onMouseDown={closeMenu} onContextMenu={(e) => e.preventDefault()} />
          <div
            role="menu"
            data-testid="terminal-menu"
            className="fixed z-50 min-w-40 rounded border border-[#2B323A] bg-[#181C20] py-1 text-xs text-[#E5E9EF] shadow-xl"
            style={{ left: menu.x, top: menu.y }}
          >
            <MenuItem
              label="Copy"
              shortcut="Ctrl+Shift+C"
              disabled={!termRef.current?.hasSelection()}
              onClick={() => {
                actionsRef.current?.copy();
                closeMenu();
              }}
            />
            <MenuItem
              label="Paste"
              shortcut="Ctrl+Shift+V"
              onClick={() => {
                actionsRef.current?.paste();
                closeMenu();
              }}
            />
            <MenuItem
              label="Select all"
              onClick={() => {
                termRef.current?.selectAll();
                closeMenu();
              }}
            />
            <MenuItem
              label="Clear scrollback"
              onClick={() => {
                termRef.current?.clear();
                closeMenu();
              }}
            />
          </div>
        </>
      )}

      {pendingPaste !== null && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div
            role="dialog"
            aria-label="Confirm paste"
            data-testid="paste-review"
            className="w-[520px] max-w-[92vw] rounded-lg border border-[#2B323A] bg-[#181C20] p-4 text-sm text-[#E5E9EF] shadow-xl"
          >
            <h3 className="mb-2 font-semibold">
              Paste {pendingPaste.split(/\r?\n/).length} lines into this session?
            </h3>
            <pre className="max-h-40 overflow-auto rounded border border-[#2B323A] bg-[#111418] p-2 font-mono text-xs whitespace-pre-wrap break-all">
              {pendingPaste.length > 4000
                ? `${pendingPaste.slice(0, 4000)}\n… (${pendingPaste.length - 4000} more chars)`
                : pendingPaste}
            </pre>
            <label className="mt-2 flex items-center gap-2 text-xs">
              <input
                type="checkbox"
                checked={paced}
                onChange={(e) => setPaced(e.target.checked)}
              />
              Send line by line with a short delay (for slow devices)
            </label>
            <div className="mt-3 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setPendingPaste(null)}
                className="rounded border border-[#2B323A] px-3 py-1 hover:bg-[#1D2228]"
              >
                Cancel
              </button>
              <button
                type="button"
                data-testid="paste-confirm"
                onClick={() => {
                  if (paced) {
                    sendPaced(pendingPaste);
                  } else {
                    void connectionWrite(sessionId, toCr(pendingPaste)).catch(() => {});
                  }
                  setPendingPaste(null);
                }}
                className="rounded bg-[#4F8EE8] px-3 py-1 font-medium text-white hover:bg-[#3F7ED8]"
              >
                Paste
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

function MenuItem({
  label,
  shortcut,
  disabled,
  onClick,
}: {
  label: string;
  shortcut?: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      onClick={onClick}
      className="flex w-full items-center gap-4 px-3 py-1 text-left hover:bg-[#2B323A] disabled:opacity-40"
    >
      <span className="flex-1">{label}</span>
      {shortcut && <span className="text-[10px] text-[#5C6370]">{shortcut}</span>}
    </button>
  );
}
