import { useMemo, useState } from "react";

import { useLibrary } from "../../state/library";
import type { Command, CommandInput } from "../../types";

interface Props {
  onUse: (command: Command, mode: "insert" | "send") => void;
}

const VENDORS = ["", "cisco", "juniper", "huawei", "mikrotik", "zte", "linux"];

export function CommandManagerPanel({ onUse }: Props) {
  const commands = useLibrary((s) => s.commands);
  const commandGroups = useLibrary((s) => s.commandGroups);
  const saveCommand = useLibrary((s) => s.saveCommand);
  const deleteCommand = useLibrary((s) => s.deleteCommand);
  const saveCommandGroup = useLibrary((s) => s.saveCommandGroup);
  const deleteCommandGroup = useLibrary((s) => s.deleteCommandGroup);

  const [filter, setFilter] = useState("");
  const [editing, setEditing] = useState<CommandInput | null>(null);

  const filtered = useMemo(() => {
    const q = filter.toLowerCase();
    return commands.filter(
      (c) =>
        !q ||
        c.name.toLowerCase().includes(q) ||
        c.content.toLowerCase().includes(q) ||
        (c.vendorProfile ?? "").toLowerCase().includes(q) ||
        c.tags.some((t) => t.toLowerCase().includes(q)),
    );
  }, [commands, filter]);

  const startNew = () => setEditing({ name: "", content: "", executeMode: "insert", riskLevel: "low", tags: [], variables: {}, vendorProfile: null, groupId: null });

  const submit = async () => {
    if (!editing) return;
    await saveCommand(editing);
    setEditing(null);
  };

  const field =
    "w-full rounded border border-[#2B323A] bg-[#111418] px-2 py-1 text-xs text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  return (
    <div data-testid="command-manager" className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-1 border-b border-[#2B323A] px-2 py-1.5">
        <span className="text-[11px] font-semibold uppercase tracking-wide text-[#99A3AE]">
          Commands
        </span>
        <div className="ml-auto flex gap-1">
          <button
            type="button"
            title="New command"
            data-testid="new-command"
            onClick={startNew}
            className="rounded px-1.5 text-[#99A3AE] hover:bg-[#2B323A] hover:text-[#E5E9EF]"
          >
            +
          </button>
          <button
            type="button"
            title="New group"
            onClick={() => {
              const name = window.prompt("Group name");
              if (name) void saveCommandGroup({ name });
            }}
            className="rounded px-1.5 text-[#99A3AE] hover:bg-[#2B323A] hover:text-[#E5E9EF]"
          >
            ▤
          </button>
        </div>
      </div>
      <div className="border-b border-[#2B323A] px-2 py-1.5">
        <input
          aria-label="Filter commands"
          data-testid="command-filter"
          placeholder="Filter..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className={field}
        />
      </div>

      {commandGroups.length > 0 && (
        <div className="border-b border-[#2B323A] px-2 py-1 text-[10px] text-[#5C6370]">
          {commandGroups.map((g) => (
            <span key={g.id} className="mr-2 inline-flex items-center gap-1">
              {g.name}
              <button
                type="button"
                aria-label={`Delete group ${g.name}`}
                onClick={() => {
                  if (window.confirm(`Delete group "${g.name}"?`)) {
                    void deleteCommandGroup(g.id);
                  }
                }}
                className="text-[#99A3AE] hover:text-red-400"
              >
                ✕
              </button>
            </span>
          ))}
        </div>
      )}

      <div className="flex-1 overflow-y-auto py-1">
        {filtered.length === 0 && (
          <p className="px-3 py-2 text-xs text-[#5C6370]">No commands yet.</p>
        )}
        {filtered.map((command) => (
          <div
            key={command.id}
            data-testid="command-item"
            data-command-name={command.name}
            className="group px-2 py-1 text-xs hover:bg-[#1D2228]"
          >
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate text-[#C7CDD6]">
                {command.name}
              </span>
              {command.vendorProfile && (
                <span className="text-[10px] uppercase text-[#5C6370]">
                  {command.vendorProfile}
                </span>
              )}
              <button
                type="button"
                title="Insert into composer"
                data-testid="command-insert"
                className="hidden px-1 hover:text-[#E5E9EF] group-hover:inline"
                onClick={() => onUse(command, "insert")}
              >
                ↳
              </button>
              <button
                type="button"
                title="Edit"
                onClick={() =>
                  setEditing({
                    id: command.id,
                    groupId: command.groupId,
                    name: command.name,
                    content: command.content,
                    vendorProfile: command.vendorProfile,
                    executeMode: command.executeMode,
                    riskLevel: command.riskLevel,
                    variables: command.variables,
                    tags: command.tags,
                  })
                }
                className="hidden px-1 hover:text-[#E5E9EF] group-hover:inline"
              >
                ✎
              </button>
              <button
                type="button"
                title="Delete"
                onClick={() => {
                  if (window.confirm(`Delete command "${command.name}"?`))
                    void deleteCommand(command.id);
                }}
                className="hidden px-1 hover:text-red-400 group-hover:inline"
              >
                ✕
              </button>
            </div>
            <div className="truncate font-mono text-[10px] text-[#5C6370]">
              {command.content}
            </div>
          </div>
        ))}
      </div>

      {editing && (
        <div className="absolute inset-0 z-30 flex items-center justify-center bg-black/50">
          <div
            role="dialog"
            aria-label="Command editor"
            className="w-[420px] rounded-lg border border-[#2B323A] bg-[#181C20] p-3 text-xs text-[#E5E9EF] shadow-xl"
          >
            <h3 className="mb-2 font-semibold">
              {editing.id ? "Edit command" : "New command"}
            </h3>
            <div className="flex flex-col gap-2">
              <input
                className={field}
                placeholder="Name"
                data-testid="command-name"
                value={editing.name}
                onChange={(e) => setEditing({ ...editing, name: e.target.value })}
              />
              <textarea
                className={`${field} h-20 font-mono`}
                placeholder="Command content"
                data-testid="command-content"
                value={editing.content}
                onChange={(e) => setEditing({ ...editing, content: e.target.value })}
              />
              <div className="flex gap-2">
                <select
                  className={field}
                  value={editing.vendorProfile ?? ""}
                  onChange={(e) =>
                    setEditing({ ...editing, vendorProfile: e.target.value || null })
                  }
                >
                  {VENDORS.map((v) => (
                    <option key={v} value={v}>
                      {v || "(no vendor)"}
                    </option>
                  ))}
                </select>
                <select
                  className={field}
                  value={editing.executeMode}
                  onChange={(e) =>
                    setEditing({ ...editing, executeMode: e.target.value })
                  }
                >
                  <option value="insert">Insert</option>
                  <option value="send">Send</option>
                </select>
                <select
                  className={field}
                  value={editing.groupId ?? ""}
                  onChange={(e) =>
                    setEditing({ ...editing, groupId: e.target.value || null })
                  }
                >
                  <option value="">(no group)</option>
                  {commandGroups.map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.name}
                    </option>
                  ))}
                </select>
              </div>
              <input
                className={field}
                placeholder="Tags (comma separated)"
                value={(editing.tags ?? []).join(", ")}
                onChange={(e) =>
                  setEditing({
                    ...editing,
                    tags: e.target.value
                      .split(",")
                      .map((t) => t.trim())
                      .filter(Boolean),
                  })
                }
              />
            </div>
            <div className="mt-3 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setEditing(null)}
                className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]"
              >
                Cancel
              </button>
              <button
                type="button"
                data-testid="command-save"
                onClick={() => void submit()}
                className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8]"
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
