import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { formatClock } from "../lib/tauri";

export interface Row {
  id: number;
  start_ms: number;
  text: string;
  edited?: boolean;
}

interface Props {
  rows: Row[];
  partial?: { text: string; start_ms: number } | null;
  emptyMessage: React.ReactNode;
  onEdit?: (id: number, text: string) => void;
}

/** How close to the bottom (px) still counts as "following the newest text". */
const STICK_THRESHOLD = 48;

export default function TranscriptView({ rows, partial, emptyMessage, onEdit }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [stick, setStick] = useState(true);
  const [unread, setUnread] = useState(0);
  const [editing, setEditing] = useState<number | null>(null);
  const lastCount = useRef(rows.length);

  const items = partial
    ? [...rows, { id: -1, start_ms: partial.start_ms, text: partial.text }]
    : rows;

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 44,
    overscan: 12,
    getItemKey: (i) => items[i].id || `partial-${i}`,
  });

  const scrollToEnd = useCallback(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
    setUnread(0);
  }, []);

  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_THRESHOLD;
    setStick(atBottom);
    if (atBottom) setUnread(0);
  }, []);

  useLayoutEffect(() => {
    const added = rows.length - lastCount.current;
    lastCount.current = rows.length;
    if (stick) {
      // rAF so the new row's height is measured before we scroll.
      requestAnimationFrame(scrollToEnd);
    } else if (added > 0) {
      setUnread((u) => u + added);
    }
  }, [rows.length, partial?.text, stick, scrollToEnd]);

  useEffect(() => {
    if (editing !== null) setStick(false);
  }, [editing]);

  if (items.length === 0) {
    return <div className="empty">{emptyMessage}</div>;
  }

  const commit = (id: number, el: HTMLElement) => {
    const text = el.innerText.trim();
    setEditing(null);
    const original = rows.find((r) => r.id === id)?.text;
    if (onEdit && text && text !== original) onEdit(id, text);
    else if (original) el.innerText = original;
  };

  return (
    <>
      <div className="scroll" ref={scrollRef} onScroll={onScroll}>
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((v) => {
            const item = items[v.index];
            const isPartial = item.id === -1;
            return (
              <div
                key={v.key}
                data-index={v.index}
                ref={virtualizer.measureElement}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${v.start}px)`,
                }}
              >
                <div
                  className={isPartial ? "line partial" : "line"}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    void navigator.clipboard.writeText(
                      `[${formatClock(item.start_ms)}] ${item.text}`,
                    );
                  }}
                  title="Right-click to copy this line"
                >
                  <span className="time">{formatClock(item.start_ms)}</span>
                  <span
                    className="text"
                    contentEditable={editing === item.id}
                    suppressContentEditableWarning
                    onDoubleClick={() => {
                      if (onEdit && !isPartial) setEditing(item.id);
                    }}
                    onBlur={(e) => editing === item.id && commit(item.id, e.currentTarget)}
                    onKeyDown={(e) => {
                      if (editing !== item.id) return;
                      if (e.key === "Enter") {
                        e.preventDefault();
                        commit(item.id, e.currentTarget);
                      } else if (e.key === "Escape") {
                        e.preventDefault();
                        e.currentTarget.innerText = item.text;
                        setEditing(null);
                      }
                    }}
                  >
                    {item.text}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {!stick && (
        <button className="jump" onClick={scrollToEnd}>
          ↓ teks terbaru{unread > 0 ? ` (${unread})` : ""}
        </button>
      )}
    </>
  );
}
