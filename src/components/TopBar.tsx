import { dbToUnit, useStore } from "../store/session";
import { formatClock } from "../lib/tauri";
import type { LangMode } from "../lib/types";
import { Clock, Pin, Sliders, TextLarger, TextSmaller } from "./Icons";

function Meter({ db, kind, title }: { db: number; kind: string; title: string }) {
  return (
    <div className={`meter ${kind}`} title={title}>
      <i style={{ width: `${dbToUnit(db) * 100}%` }} />
    </div>
  );
}

export default function TopBar() {
  const {
    recording,
    elapsedMs,
    settings,
    sources,
    levels,
    view,
    toggleRecording,
    setView,
    saveSettings,
    setFontSize,
    toggleAlwaysOnTop,
  } = useStore();

  const systemSource = sources.find((s) => s.id === settings?.system_source_id);

  return (
    <div className="topbar" data-tauri-drag-region="">
      <button
        className={`rec-btn ${recording ? "active" : ""}`}
        onClick={() => void toggleRecording()}
        title={recording ? "Stop recording (⌘⇧R)" : "Start recording (⌘⇧R)"}
      >
        <span className="rec-dot" />
        <span>{recording ? "Stop" : "Start"}</span>
      </button>

      <span className="clock">{formatClock(elapsedMs)}</span>

      {recording && (
        <>
          <Meter db={levels.system} kind="sys" title="System audio level" />
          {settings?.mic_transcribe && (
            <Meter db={levels.mic} kind="mic" title="Microphone level (recorded, never shown on screen)" />
          )}
        </>
      )}

      <span className="chip" title={systemSource?.name ?? "No source selected"}>
        {systemSource?.name ?? "No source selected"}
      </span>

      <select
        value={settings?.lang_mode ?? "auto"}
        onChange={(e) => void saveSettings({ lang_mode: e.target.value as LangMode })}
        title="Meeting language — Auto detects English and Indonesian"
        style={{ fontSize: 12, padding: "3px 6px" }}
      >
        <option value="auto">Auto</option>
        <option value="en">EN</option>
        <option value="id">ID</option>
      </select>

      <div className="spacer" data-tauri-drag-region="" />

      <button className="icon-btn size-btn" onClick={() => void setFontSize(-2)} title="Smaller text (⌘−)">
        <TextSmaller />
      </button>
      <button className="icon-btn size-btn" onClick={() => void setFontSize(2)} title="Larger text (⌘+)">
        <TextLarger />
      </button>
      <button
        className={`icon-btn ${settings?.always_on_top ? "on" : ""}`}
        onClick={() => void toggleAlwaysOnTop()}
        title="Keep window on top"
      >
        <Pin />
      </button>
      <button
        className={`icon-btn ${view === "history" ? "on" : ""}`}
        onClick={() => setView(view === "history" ? "live" : "history")}
        title="Meeting history"
      >
        <Clock />
      </button>
      <button
        className={`icon-btn ${view === "settings" ? "on" : ""}`}
        onClick={() => setView(view === "settings" ? "live" : "settings")}
        title="Settings"
      >
        <Sliders />
      </button>
    </div>
  );
}
