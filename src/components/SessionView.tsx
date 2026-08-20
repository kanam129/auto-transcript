import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api, errorMessage, formatClock, formatDate } from "../lib/tauri";
import { useStore } from "../store/session";
import TranscriptView from "./TranscriptView";
import { ArrowLeft, Captions, Download, Mic, Trash, User } from "./Icons";

export default function SessionView() {
  const { openSession, closeSession, openSessionById, notify, applyEditedSegment } = useStore();
  const [includeMic, setIncludeMic] = useState(false);
  const [busy, setBusy] = useState(false);

  if (!openSession) return null;
  const { meta, segments } = openSession;

  const doExport = async (format: string) => {
    try {
      const path = await save({
        defaultPath: `${meta.title.replace(/[/:]/g, "-")}.${format}`,
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
      });
      if (!path) return;
      await api.exportSession(meta.id, format, path, true);
      notify({ kind: "warn", title: "Transcript exported", detail: path });
    } catch (e) {
      notify({ kind: "error", title: "Export failed", detail: errorMessage(e) });
    }
  };

  const doDelete = async () => {
    if (!confirm(`Delete "${meta.title}" and its audio recording?`)) return;
    try {
      await api.deleteSession(meta.id, true);
      closeSession();
    } catch (e) {
      notify({ kind: "error", title: "Could not delete", detail: errorMessage(e) });
    }
  };

  const doRetranscribeMic = async () => {
    setBusy(true);
    try {
      await api.transcribeMic(meta.id);
      notify({
        kind: "warn",
        title: "Re-transcribing the microphone track",
        detail: "It runs in the background — reopen this session shortly to see the result.",
      });
    } catch (e) {
      notify({ kind: "error", title: "Re-transcription failed", detail: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="panel">
      <div className="panel-head">
        <button className="icon-btn" onClick={closeSession} title="Back to history">
          <ArrowLeft />
        </button>
        <div style={{ minWidth: 0 }}>
          <input
            defaultValue={meta.title}
            onBlur={(e) => {
              const t = e.target.value.trim();
              if (t && t !== meta.title) void api.renameSession(meta.id, t);
            }}
            style={{ fontSize: 14, fontWeight: 600, width: 280 }}
          />
          <div style={{ fontSize: 11, color: "var(--muted)", marginTop: 3 }}>
            {formatDate(meta.started_at)}
            {meta.duration_ms ? ` · ${formatClock(meta.duration_ms)}` : ""} ·{" "}
            {meta.model_used ?? "?"} · {meta.source_name ?? "?"}
          </div>
        </div>
        <div className="spacer" />
        <div className="actions">
        <button
          className={`icon-btn ${includeMic ? "on" : ""}`}
          title={includeMic ? "Hide my own voice" : "Show my own voice too"}
          onClick={() => {
            const next = !includeMic;
            setIncludeMic(next);
            void openSessionById(meta.id, next);
          }}
        >
          <User />
        </button>
        <button className="icon-btn" onClick={() => void doExport("md")} title="Export as Markdown">
          <Download />
        </button>
        <button className="icon-btn" onClick={() => void doExport("srt")} title="Export as SRT subtitles">
          <Captions />
        </button>
        {meta.mic_wav_path && (
          <button
            className="icon-btn"
            disabled={busy}
            onClick={() => void doRetranscribeMic()}
            title="Re-transcribe the microphone recording"
          >
            <Mic />
          </button>
        )}
        <button className="icon-btn danger" onClick={() => void doDelete()} title="Delete this session">
          <Trash />
        </button>
        </div>
      </div>

      <div className="body">
        <TranscriptView
          rows={segments.map((s) => ({
            id: s.id,
            start_ms: s.start_ms,
            text: includeMic ? `${s.track === "mic" ? "You: " : ""}${s.text}` : s.text,
            edited: s.edited,
          }))}
          emptyMessage="This session produced no transcript."
          onEdit={(id, text) => {
            void api
              .editSegment(id, text)
              .then(() => applyEditedSegment(id, text))
              .catch((e) =>
                notify({ kind: "error", title: "Could not save the correction", detail: errorMessage(e) }),
              );
          }}
        />
      </div>
    </div>
  );
}
