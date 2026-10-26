import { useLibrary } from "../../state/library";
import type { Command } from "../../types";

interface Props {
  onUse: (command: Command, mode: "insert" | "send") => void;
}

/**
 * Compact configurable button bar (FR-11). Each button is derived from a saved
 * command; the label distinguishes `Insert` from `Send` so an action is never
 * ambiguous (default is Insert).
 */
export function ButtonBar({ onUse }: Props) {
  const commands = useLibrary((s) => s.commands);
  if (commands.length === 0) return null;

  return (
    <div
      data-testid="button-bar"
      className="flex flex-wrap items-center gap-1 border-b border-[#2B323A] bg-[#15181C] px-2 py-1"
    >
      {commands.slice(0, 24).map((command) => {
        const mode = command.executeMode === "send" ? "send" : "insert";
        return (
          <button
            key={command.id}
            type="button"
            title={command.content}
            data-testid="button-bar-item"
            onClick={() => onUse(command, mode)}
            className={`rounded border px-2 py-0.5 text-[11px] ${
              mode === "send"
                ? "border-amber-500/50 text-amber-300 hover:bg-[#2a2416]"
                : "border-[#2B323A] text-[#C7CDD6] hover:bg-[#1D2228]"
            }`}
          >
            {mode === "send" ? "▶ " : "↳ "}
            {command.name}
          </button>
        );
      })}
    </div>
  );
}
