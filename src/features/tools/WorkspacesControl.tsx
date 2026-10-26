import { useEffect, useState } from "react";

import { workspaceDelete, workspaceSave, workspacesList } from "../../lib/ipc";
import { useSessions } from "../../state/sessions";
import type { Workspace } from "../../types";

export interface PaneDescriptor {
  kind: "saved" | "local";
  savedId?: string;
}

export interface WorkspaceTabLayout {
  orientation: "row" | "column";
  panes: PaneDescriptor[];
}

export interface WorkspaceLayout {
  tabs: WorkspaceTabLayout[];
}

interface Props {
  onRestore: (layout: WorkspaceLayout) => void;
}

export function WorkspacesControl({ onRestore }: Props) {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selected, setSelected] = useState("");

  const refresh = () => void workspacesList().then(setWorkspaces).catch(() => {});
  useEffect(refresh, []);

  const save = async () => {
    const name = window.prompt("Workspace name");
    if (!name) return;
    const tabs = useSessions.getState().tabs;
    const layout: WorkspaceLayout = {
      tabs: tabs.map((t) => ({
        orientation: t.orientation,
        panes: t.panes.map((_, i) =>
          i === 0 && t.savedId
            ? { kind: "saved", savedId: t.savedId }
            : { kind: "local" },
        ),
      })),
    };
    await workspaceSave({ name, layout: layout as unknown as Record<string, unknown> });
    refresh();
  };

  const restore = () => {
    const ws = workspaces.find((w) => w.id === selected);
    if (!ws) return;
    const layout = ws.layout as unknown as WorkspaceLayout;
    const tabs = layout?.tabs ?? [];
    if (tabs.length === 0) {
      window.alert("This workspace is empty.");
      return;
    }
    const count = tabs.reduce((n, t) => n + t.panes.length, 0);
    if (
      !window.confirm(
        `Restore workspace "${ws.name}"? This opens ${count} pane(s) as new connections.`,
      )
    ) {
      return;
    }
    onRestore(layout);
  };

  const field =
    "rounded border border-[#2B323A] bg-[#111418] px-2 py-0.5 text-xs text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  return (
    <div className="flex items-center gap-1">
      <select
        className={field}
        value={selected}
        onChange={(e) => setSelected(e.target.value)}
        aria-label="Workspaces"
      >
        <option value="">Workspaces…</option>
        {workspaces.map((w) => (
          <option key={w.id} value={w.id}>
            {w.name}
          </option>
        ))}
      </select>
      <button
        type="button"
        disabled={!selected}
        onClick={restore}
        className="rounded border border-[#2B323A] px-2 py-0.5 text-xs hover:bg-[#1D2228] disabled:opacity-40"
      >
        Restore
      </button>
      <button
        type="button"
        onClick={() => void save()}
        data-testid="workspace-save"
        className="rounded border border-[#2B323A] px-2 py-0.5 text-xs hover:bg-[#1D2228]"
      >
        Save
      </button>
      {selected && (
        <button
          type="button"
          onClick={() =>
            void workspaceDelete(selected).then(() => {
              setSelected("");
              refresh();
            })
          }
          className="rounded border border-[#2B323A] px-2 py-0.5 text-xs hover:bg-[#1D2228] hover:text-red-400"
        >
          Delete
        </button>
      )}
    </div>
  );
}
