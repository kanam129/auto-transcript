import { useEffect, useState } from "react";
import { api, errorMessage, formatBytes } from "../lib/tauri";
import type { DataPaths } from "../lib/types";
import { useStore } from "../store/session";
import { ArrowLeft } from "./Icons";
import ProfilePicker from "./ProfilePicker";

export default function SettingsPanel() {
  const {
    settings,
    sources,
    models,
    downloads,
    setView,
    saveSettings,
    refreshSources,
    refreshModels,
    notify,
  } = useStore();
  const [usage, setUsage] = useState<{ bytes: number; sessions: number } | null>(null);
  const [paths, setPaths] = useState<DataPaths | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [hasKey, setHasKey] = useState(false);

  useEffect(() => {
    api.diskUsage().then(setUsage).catch(() => undefined);
    api.hasLlmApiKey().then(setHasKey).catch(() => undefined);
    api.dataPaths().then(setPaths).catch(() => undefined);
  }, []);

  if (!settings) return null;

  const download = async (id: string) => {
    try {
      await api.downloadModel(id);
      await refreshModels();
      notify({ kind: "warn", title: `Model ${id} is ready` });
    } catch (e) {
      notify({ kind: "error", title: "Model download failed", detail: errorMessage(e) });
    }
  };

  return (
    <div className="panel">
      <div className="panel-head">
        <button className="icon-btn" onClick={() => setView("live")} title="Back">
          <ArrowLeft />
        </button>
        <h2>Settings</h2>
      </div>

      <div className="panel-body">
        <h3 className="section">Audio sources</h3>

        <div className="field">
          <label>System audio — this is what gets transcribed and shown</label>
          <div className="row">
            <select
              value={settings.system_source_id ?? ""}
              onChange={(e) => void saveSettings({ system_source_id: e.target.value || null })}
              style={{ flex: 1 }}
            >
              <option value="">— choose a device —</option>
              {sources
                .filter((s) => s.kind !== "mic")
                .map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                    {s.kind === "system_output" ? "  — direct capture" : "  — virtual driver"}
                  </option>
                ))}
            </select>
            <button className="btn" onClick={() => void refreshSources()}>
              Refresh
            </button>
          </div>
          <span className="hint">
            Pick the device your meeting audio plays through (headphones or speakers). The ones
            marked "direct capture" need no driver at all and do not change your sound settings —
            your volume keys keep working.
          </span>
        </div>

        <div className="field">
          <label>Microphone — recorded and transcribed, but never shown on screen</label>
          <select
            value={settings.mic_source_id ?? ""}
            onChange={(e) => void saveSettings({ mic_source_id: e.target.value || null })}
          >
            <option value="">— not used —</option>
            {sources
              .filter((s) => s.kind === "mic")
              .map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
          </select>
          <label className="row" style={{ marginTop: 4 }}>
            <input
              type="checkbox"
              checked={settings.mic_transcribe}
              onChange={(e) => void saveSettings({ mic_transcribe: e.target.checked })}
            />
            <span className="hint" style={{ margin: 0 }}>
              Transcribe my voice too (used as material for AI summaries later; never appears in
              the transcript window)
            </span>
          </label>
        </div>

        <h3 className="section">Transcription quality</h3>
        <ProfilePicker />
        <span className="hint" style={{ display: "block", margin: "8px 0 4px" }}>
          Each level sets both models the app uses: a large one for the final text and a small
          one for the live preview. Which pairing works came out of measurements — see
          BENCHMARK.md.
        </span>

        <h3 className="section">Models, one by one</h3>
        {models.map((m) => {
          const dl = downloads[m.id];
          const pct = dl && dl.total > 0 ? Math.round((dl.downloaded / dl.total) * 100) : 0;
          const busy = dl && dl.downloaded < dl.total;
          return (
            <div key={m.id} className="model-row">
              <input
                type="radio"
                name="model"
                checked={settings.model_id === m.id}
                disabled={!m.downloaded}
                onChange={() => void saveSettings({ model_id: m.id })}
              />
              <div className="grow">
                <div>
                  {m.name} <span style={{ color: "var(--muted)" }}>{formatBytes(m.size_bytes)}</span>
                </div>
                <div className="note">{m.note}</div>
                {busy && (
                  <div className="progress">
                    <i style={{ width: `${pct}%` }} />
                  </div>
                )}
              </div>
              {m.downloaded ? (
                <span className="chip">installed</span>
              ) : (
                <button className="btn" disabled={busy} onClick={() => void download(m.id)}>
                  {busy ? `${pct}%` : "Download"}
                </button>
              )}
            </div>
          );
        })}

        <div className="field" style={{ marginTop: 16 }}>
          <label>Preview model — draws the grey text while someone is still speaking</label>
          <select
            value={settings.partial_model_id}
            onChange={(e) => void saveSettings({ partial_model_id: e.target.value })}
          >
            {models
              .filter((m) => m.downloaded)
              .map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))}
          </select>
          <span className="hint">
            Kept separate from the main model so preview text stays quick. Smaller is faster but
            makes more mistakes — and those mistakes are what you read during the meeting.
          </span>
        </div>

        <div className="field">
          <label className="row">
            <input
              type="checkbox"
              checked={settings.partials_enabled}
              onChange={(e) => void saveSettings({ partials_enabled: e.target.checked })}
            />
            <span>Show preview text while someone is still speaking</span>
          </label>
          <span className="hint">
            Uses a separate smaller model so the final text is not delayed. Turn it off if your
            machine feels strained.
          </span>
        </div>

        <div className="field">
          <label>Custom vocabulary</label>
          <textarea
            rows={3}
            defaultValue={settings.vocabulary}
            placeholder="Participant names, product names, internal acronyms — comma separated"
            onBlur={(e) => void saveSettings({ vocabulary: e.target.value })}
          />
          <span className="hint">
            Passed to Whisper as an initial prompt. It noticeably improves accuracy on names and
            in-house terminology.
          </span>
        </div>

        <h3 className="section">Storage</h3>
        <div className="field">
          <label>Automatically delete audio recordings older than</label>
          <div className="row">
            <select
              value={settings.keep_audio_days ?? ""}
              onChange={(e) =>
                void saveSettings({
                  keep_audio_days: e.target.value ? Number(e.target.value) : null,
                })
              }
            >
              <option value="">Never</option>
              <option value="7">7 days</option>
              <option value="30">30 days</option>
              <option value="90">90 days</option>
            </select>
            <button
              className="btn"
              onClick={() =>
                void api.cleanupOldAudio().then((freed) => {
                  notify({
                    kind: "warn",
                    title: freed > 0 ? `${formatBytes(freed)} freed` : "Nothing to delete",
                  });
                  api.diskUsage().then(setUsage).catch(() => undefined);
                })
              }
            >
              Clean up now
            </button>
          </div>
          <span className="hint">
            Transcripts are kept forever — only the raw audio files are removed.
            {usage ? ` Currently ${formatBytes(usage.bytes)} across ${usage.sessions} sessions.` : ""}
          </span>
        </div>

        <h3 className="section">Where your data lives</h3>
        <div className="field">
          <span className="hint">
            Everything stays on this machine. Removing the app does not delete any of this — use
            the folder below if you want it gone.
          </span>
          {paths && (
            <div className="path">
              <div>
                <b>Everything</b>
                {paths.data_dir}
              </div>
              <div>
                <b>Transcripts</b>
                {paths.database}
              </div>
              <div>
                <b>Models</b>
                {paths.models_dir}
              </div>
              <div>
                <b>Recordings</b>
                {paths.recordings_dir}
              </div>
              <div>
                <b>Logs</b>
                {paths.log_dir}
              </div>
            </div>
          )}
          {paths && (
            <button
              className="btn"
              style={{ width: 150, marginTop: 6 }}
              onClick={() =>
                void api.revealInFinder(paths.database).catch((e) =>
                  notify({ kind: "error", title: "Could not open the folder", detail: errorMessage(e) }),
                )
              }
            >
              Open data folder
            </button>
          )}
        </div>

        <h3 className="section">AI summaries (not enabled)</h3>
        <div className="field">
          <label className="row">
            <input
              type="checkbox"
              checked={settings.summary_enabled}
              onChange={(e) => void saveSettings({ summary_enabled: e.target.checked })}
            />
            <span>Allow sending transcripts to an AI service</span>
          </label>
          <span className="hint">
            While this is off, the app never touches the network at all. All transcription runs
            on this machine.
          </span>
        </div>
        <div className="field">
          <label>Base URL (OpenAI-compatible)</label>
          <input
            defaultValue={settings.llm.base_url}
            onBlur={(e) => void saveSettings({ llm: { ...settings.llm, base_url: e.target.value } })}
          />
          <label style={{ marginTop: 6 }}>Model</label>
          <input
            defaultValue={settings.llm.model}
            onBlur={(e) => void saveSettings({ llm: { ...settings.llm, model: e.target.value } })}
          />
          <label style={{ marginTop: 6 }}>API key {hasKey ? "(stored in the keychain)" : ""}</label>
          <div className="row">
            <input
              type="password"
              value={apiKey}
              placeholder={hasKey ? "••••••••" : "sk-…"}
              onChange={(e) => setApiKey(e.target.value)}
              style={{ flex: 1 }}
            />
            <button
              className="btn"
              onClick={() =>
                void api
                  .setLlmApiKey(apiKey)
                  .then(() => {
                    setApiKey("");
                    return api.hasLlmApiKey().then(setHasKey);
                  })
                  .catch((e) =>
                    notify({ kind: "error", title: "Could not save the key", detail: errorMessage(e) }),
                  )
              }
            >
              Save
            </button>
          </div>
          <span className="hint">Stored in the OS keychain, never written to a file.</span>
        </div>
      </div>
    </div>
  );
}
