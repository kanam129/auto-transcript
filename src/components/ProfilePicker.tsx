import { useState } from "react";
import { api, errorMessage, formatBytes } from "../lib/tauri";
import { useStore } from "../store/session";
import type { ProfileStatus } from "../lib/types";

/**
 * The quality-level picker, used both on the first-run screen and in Settings.
 *
 * Selecting and downloading are deliberately kept apart. The Download button used to also
 * make that profile the active one, so somebody downloading a couple of options just to try
 * them ended up using whichever they clicked last — without ever being told. Now the radio
 * selects, the button downloads, and neither changes the other.
 */
export default function ProfilePicker() {
  const { settings, profiles, downloads, refreshModels, refreshProfiles, notify } = useStore();
  const [busy, setBusy] = useState<string | null>(null);

  if (!settings) return null;

  const modelsOf = (p: ProfileStatus) =>
    p.partial_model_id === p.model_id ? [p.model_id] : [p.model_id, p.partial_model_id];

  /** Combined progress across every model a profile needs. */
  const progressOf = (p: ProfileStatus) => {
    let done = 0;
    let total = 0;
    for (const id of modelsOf(p)) {
      const d = downloads[id];
      if (d) {
        done += d.downloaded;
        total += d.total;
      }
    }
    return total > 0 ? Math.round((done / total) * 100) : 0;
  };

  const select = async (p: ProfileStatus) => {
    try {
      const fresh = await api.applyProfile(p.id);
      useStore.setState({ settings: fresh });
    } catch (e) {
      notify({ kind: "error", title: "Could not switch quality", detail: errorMessage(e) });
    }
  };

  const download = async (p: ProfileStatus) => {
    setBusy(p.id);
    try {
      for (const id of modelsOf(p)) {
        await api.downloadModel(id);
      }
      await Promise.all([refreshModels(), refreshProfiles()]);
    } catch (e) {
      notify({ kind: "error", title: "Model download failed", detail: errorMessage(e) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      {profiles.map((p) => {
        const active = settings.model_id === p.model_id && settings.partial_model_id === p.partial_model_id;
        const pct = progressOf(p);
        const downloading = busy === p.id;
        return (
          <label key={p.id} className={`profile ${active ? "active" : ""}`}>
            <input
              type="radio"
              name="quality-profile"
              checked={active}
              disabled={!!busy}
              onChange={() => void select(p)}
            />
            <div className="grow">
              <div className="profile-title">
                {p.name}
                {p.recommended && <span className="tag">recommended</span>}
                <span className="size">
                  {p.ready ? "installed" : `${formatBytes(p.download_bytes)} to download`}
                </span>
              </div>
              <div className="note">{p.note}</div>
              {downloading && (
                <div className="progress">
                  <i style={{ width: `${pct}%` }} />
                </div>
              )}
            </div>
            {!p.ready && (
              <button
                className="btn"
                disabled={!!busy}
                onClick={(e) => {
                  e.preventDefault();
                  void download(p);
                }}
              >
                {downloading ? `${pct}%` : "Download"}
              </button>
            )}
          </label>
        );
      })}
    </>
  );
}
