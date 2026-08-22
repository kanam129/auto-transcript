import { useEffect } from "react";
import { teardown, useStore } from "./store/session";
import TopBar from "./components/TopBar";
import LiveCaption from "./components/LiveCaption";
import HistoryPanel from "./components/HistoryPanel";
import SessionView from "./components/SessionView";
import SettingsPanel from "./components/SettingsPanel";
import Notices from "./components/Notices";
import Prerequisites from "./components/Prerequisites";
import Onboarding from "./components/Onboarding";

export default function App() {
  const {
    ready,
    settings,
    view,
    recording,
    segments,
    partial,
    platform,
    init,
    setFontSize,
    toggleRecording,
  } = useStore();

  useEffect(() => {
    void init();
    return teardown;
  }, [init]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Command on macOS, Control everywhere else. `metaKey` on Windows is the Windows
      // key, so testing it alone left every one of these shortcuts dead there.
      const modifier = platform === "macos" ? e.metaKey : e.ctrlKey;
      if (!modifier) return;
      if (e.key === "=" || e.key === "+") {
        e.preventDefault();
        void setFontSize(2);
      } else if (e.key === "-") {
        e.preventDefault();
        void setFontSize(-2);
      } else if (e.key === "0") {
        e.preventDefault();
        void setFontSize("reset");
      } else if (e.shiftKey && (e.key === "r" || e.key === "R")) {
        e.preventDefault();
        void toggleRecording();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [platform, setFontSize, toggleRecording]);

  if (!ready) {
    return <div className="empty">Starting up…</div>;
  }

  return (
    <div className="app">
      <TopBar />
      <div className="body">
        <Prerequisites />
        <Notices />
        <LiveCaption
          segments={segments}
          partial={partial}
          empty={
            recording
              ? "Listening…"
              : "Press Start to record a meeting.\nOnly audio coming out of this computer is shown here."
          }
        />
        {settings && !settings.onboarded && <Onboarding />}
        {view === "history" && <HistoryPanel />}
        {view === "session" && <SessionView />}
        {view === "settings" && <SettingsPanel />}
      </div>
    </div>
  );
}
