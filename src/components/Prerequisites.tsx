import { useStore } from "../store/session";

/**
 * Warnings about things currently preventing a recording.
 *
 * Deliberately recomputed from state on every render rather than pushed as a notification.
 * Both of these used to be emitted once at startup, which left "Model not downloaded" stuck
 * on screen after the user had finished downloading the model — the message was true when it
 * was created, then became false with nothing to withdraw it.
 *
 * The rule: events (a device disappearing, a model downgrade) become notifications;
 * conditions (whether a model or an audio source exists) are computed like this.
 */
export default function Prerequisites() {
  const { settings, models, sources, setView } = useStore();

  // The first-run screen already handles both of these in its own way.
  if (!settings || !settings.onboarded) return null;

  const modelMissing = !models.some((m) => m.id === settings.model_id && m.downloaded);
  const noSystemSource = sources.length > 0 && !sources.some((s) => s.kind !== "mic");

  if (!modelMissing && !noSystemSource) return null;

  return (
    <>
      {modelMissing && (
        <div className="banner warn">
          <div>
            <strong>Transcription model not installed</strong>
            <div>
              The selected model ({settings.model_id}) is not on this machine yet, so recording
              will fail.
            </div>
          </div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={() => setView("settings")}>
            Open Settings
          </button>
        </div>
      )}
      {noSystemSource && (
        <div className="banner warn">
          <div>
            <strong>No system audio source</strong>
            <div>No output device could be captured. Check your system sound settings.</div>
          </div>
        </div>
      )}
    </>
  );
}
