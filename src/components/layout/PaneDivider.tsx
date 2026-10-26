import { useRef } from "react";

interface Props {
  orientation: "row" | "column";
  /** Fraction of the container moved from the right/bottom pane to the left/top. */
  onDrag: (fraction: number) => void;
}

/** A draggable divider between two split panes. */
export function PaneDivider({ orientation, onDrag }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const horizontal = orientation === "row";
  return (
    <div
      ref={ref}
      role="separator"
      aria-orientation={horizontal ? "vertical" : "horizontal"}
      className={
        horizontal
          ? "w-1 shrink-0 cursor-col-resize bg-[#2B323A] hover:bg-[#4F8EE8]"
          : "h-1 shrink-0 cursor-row-resize bg-[#2B323A] hover:bg-[#4F8EE8]"
      }
      onPointerDown={(event) => {
        event.preventDefault();
        const parent = ref.current?.parentElement;
        if (!parent) return;
        const rect = parent.getBoundingClientRect();
        const total = horizontal ? rect.width : rect.height;
        if (total <= 0) return;
        let last = horizontal ? event.clientX : event.clientY;
        const move = (ev: PointerEvent) => {
          const current = horizontal ? ev.clientX : ev.clientY;
          onDrag((current - last) / total);
          last = current;
        };
        const up = () => {
          window.removeEventListener("pointermove", move);
          window.removeEventListener("pointerup", up);
        };
        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", up);
      }}
    />
  );
}
