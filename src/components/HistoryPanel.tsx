import { useEffect, useState } from "react";
import { api, errorMessage, formatBytes, formatClock, formatDate } from "../lib/tauri";
import { useStore } from "../store/session";
import { ArrowLeft, Search } from "./Icons";
import type { SessionMeta } from "../lib/types";

/** FTS5 snippets arrive marked with « » — turn those into <mark> without injecting raw HTML. */
function Snippet({ text }: { text: string }) {
  const parts = text.split(/[«»]/);
  return (
    <div className="snippet">
      {parts.map((p, i) => (i % 2 === 1 ? <mark key={i}>{p}</mark> : <span key={i}>{p}</span>))}
    </div>
  );
}

export default function HistoryPanel() {
  const { setView, openSessionById, notify } = useStore();
  const [q, setQ] = useState("");
  const [rows, setRows] = useState<SessionMeta[]>([]);
  const [usage, setUsage] = useState<{ bytes: number; sessions: number } | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    const t = setTimeout(async () => {
      setLoading(true);
      try {
        const list = await api.listSessions(q.trim() || undefined);
        if (alive) setRows(list);
      } catch (e) {
        notify({ kind: "error", title: "Search failed", detail: errorMessage(e) });
      } finally {
        if (alive) setLoading(false);
      }
    }, 180);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [q, notify]);

  useEffect(() => {
    api.diskUsage().then(setUsage).catch(() => undefined);
  }, []);

  return (
    <div className="panel">
      <div className="panel-head">
        <button className="icon-btn" onClick={() => setView("live")} title="Back">
          <ArrowLeft />
        </button>
        <h2>History</h2>
        <div className="search-box">
          <Search size={15} />
          <input
            placeholder="Search every transcript…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            autoFocus
          />
        </div>
        {usage && (
          <span className="chip" title="Total size of stored audio">
            {usage.sessions} sessions · {formatBytes(usage.bytes)}
          </span>
        )}
      </div>

      <div className="panel-body">
        {loading && rows.length === 0 && <div className="empty">Loading…</div>}
        {!loading && rows.length === 0 && (
          <div className="empty">
            {q ? `No results for "${q}".` : "No meetings recorded yet."}
          </div>
        )}
        {rows.map((s) => (
          <button key={s.id} className="session-row" onClick={() => void openSessionById(s.id)}>
            <span className="title">{s.title}</span>
            <span className="meta">
              {formatDate(s.started_at)}
              {s.duration_ms ? ` · ${formatClock(s.duration_ms)}` : ""} · {s.word_count} words
              {s.model_used ? ` · ${s.model_used}` : ""}
            </span>
            {s.snippet && <Snippet text={s.snippet} />}
          </button>
        ))}
      </div>
    </div>
  );
}
