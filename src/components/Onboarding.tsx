import { useMemo } from "react";
import { useStore } from "../store/session";
import ProfilePicker from "./ProfilePicker";

/**
 * First-run screen: pick a quality level, download it, done.
 *
 * The choice is a quality level rather than a model name on purpose. The app always runs
 * two models — a large one for the final text and a small one for the realtime preview —
 * and which pairing makes sense came out of measurements, not taste. Asking someone to
 * work that out before their first meeting would be asking the wrong question.
 */
export default function Onboarding() {
  const { settings, sources, profiles, platform, saveSettings, refreshSources } = useStore();

  const direct = useMemo(() => sources.filter((s) => s.kind === "system_output"), [sources]);
  const virtual_ = useMemo(() => sources.filter((s) => s.kind === "virtual_loopback"), [sources]);
  const isMac = platform === "macos";

  if (!settings) return null;

  const selected = profiles.find(
    (p) => p.model_id === settings.model_id && p.partial_model_id === settings.partial_model_id,
  );

  return (
    <div className="panel">
      <div className="panel-head">
        <h2>Welcome to Auto-Transcript</h2>
      </div>
      <div className="panel-body">
        <p style={{ color: "var(--fg-dim)", fontSize: 13, lineHeight: 1.7, maxWidth: 560 }}>
          This app shows the text of whatever audio is <strong>coming out</strong> of your
          computer — the people you are meeting with. Your own microphone is still recorded and
          transcribed quietly as material for summaries, but it never appears on screen.
          Everything is transcribed locally; nothing is sent to the internet.
        </p>

        <h3 className="section">1 · Choose transcription quality {selected?.ready && "✓"}</h3>
        <ProfilePicker />
        <span className="hint" style={{ display: "block", marginTop: 8 }}>
          Downloaded once, then used entirely offline. You can change this later in Settings.
        </span>

        <h3 className="section">2 · System audio source {direct.length > 0 && "✓"}</h3>
        <div className="field">
          {direct.length > 0 || virtual_.length > 0 ? (
            <>
              <select
                value={settings.system_source_id ?? direct[0]?.id ?? virtual_[0]?.id}
                onChange={(e) => void saveSettings({ system_source_id: e.target.value })}
              >
                {[...direct, ...virtual_].map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                    {s.kind === "system_output" ? "  — direct capture" : "  — virtual driver"}
                  </option>
                ))}
              </select>
              <span className="hint">
                Pick the device your meeting audio plays through. "Direct capture" records from
                that device without any driver, without changing your sound settings, and your
                volume keys keep working.
              </span>
            </>
          ) : (
            <>
              <span style={{ fontSize: 13 }}>No output device could be captured.</span>
              <button className="btn" style={{ width: 140 }} onClick={() => void refreshSources()}>
                Check again
              </button>
            </>
          )}
        </div>

        <h3 className="section">3 · Permissions</h3>
        <div className="field">
          <span className="hint" style={{ fontSize: 12 }}>
            {isMac
              ? "The first time you record, macOS will ask for microphone and system-audio recording permission. Both need to be granted for transcription to work."
              : "The first time you record, Windows may ask for microphone permission. Capturing system audio needs no extra permission."}
          </span>
        </div>

        <button
          className="btn"
          style={{ marginTop: 8 }}
          disabled={!selected?.ready}
          onClick={() => void saveSettings({ onboarded: true })}
        >
          {selected?.ready ? "Done, start using it" : "Download a model first"}
        </button>
      </div>
    </div>
  );
}
