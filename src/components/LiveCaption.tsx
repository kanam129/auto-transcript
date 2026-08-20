import { useMemo } from "react";
import type { LiveSegment } from "../store/session";

interface Props {
  segments: LiveSegment[];
  partial: { text: string; start_ms: number } | null;
  empty: React.ReactNode;
}

/** How much history is kept in the upper zone, measured in characters. */
const HISTORY_CHARS = 900;

/** The brightest older text ever gets. Well below white, so it does not compete for the eye. */
const HISTORY_MAX_ALPHA = 0.26;
const HISTORY_MIN_ALPHA = 0.05;

function historyAlpha(charsFromEnd: number): number {
  const t = Math.min(1, charsFromEnd / HISTORY_CHARS);
  // An exponent below 1 makes the drop steep at first: once a sentence is past, it recedes
  // into the background quickly instead of lingering half-lit and still pulling the eye.
  return HISTORY_MAX_ALPHA - (HISTORY_MAX_ALPHA - HISTORY_MIN_ALPHA) * Math.pow(t, 0.45);
}

/**
 * The live view, built around a fixed reading line.
 *
 * The screen is split into two zones whose boundary never moves. The lower zone always starts
 * at the same point and holds only the sentence currently being spoken, in full white. The
 * upper zone holds earlier sentences, much dimmer, growing upward and disappearing off the
 * edge.
 *
 * Why split it at all: in a single stream anchored to the bottom, every time the running
 * sentence grows enough to need another line, all of the text — including the sentence being
 * read — is pushed upward. With two zones, a new sentence always appears at the same height
 * and grows downward, so the eye can stay in one place.
 */
export default function LiveCaption({ segments, partial, empty }: Props) {
  const { history, current } = useMemo(() => {
    const finals = segments
      .map((s) => ({ key: String(s.id), text: s.text.trim() }))
      .filter((s) => s.text.length > 0);

    // What is being spoken: the preview text if there is one, otherwise the most recent
    // final sentence, so the lower zone does not suddenly empty when someone stops talking.
    const live = partial?.text.trim();
    const current = live ? { key: "partial", text: live } : finals[finals.length - 1];
    const older = live ? finals : finals.slice(0, -1);

    const kept: { key: string; text: string }[] = [];
    let total = 0;
    for (let i = older.length - 1; i >= 0; i--) {
      total += older[i].text.length + 1;
      if (total > HISTORY_CHARS && kept.length > 0) break;
      kept.unshift(older[i]);
    }

    let charsAfter = 0;
    const shaded = new Array(kept.length);
    for (let i = kept.length - 1; i >= 0; i--) {
      shaded[i] = { ...kept[i], alpha: historyAlpha(charsAfter) };
      charsAfter += kept[i].text.length + 1;
    }
    return { history: shaded, current };
  }, [segments, partial]);

  if (!current && history.length === 0) {
    return (
      <div className="empty" data-tauri-drag-region="">
        {empty}
      </div>
    );
  }

  return (
    <div className="caption" data-tauri-drag-region="">
      <div className="caption-history" data-tauri-drag-region="">
        <p>
          {history.map((item: { key: string; text: string; alpha: number }) => (
            <span
              key={item.key}
              className="seg"
              style={{ color: `rgba(250, 250, 250, ${item.alpha.toFixed(3)})` }}
            >
              {item.text}{" "}
            </span>
          ))}
        </p>
      </div>

      <div className="caption-current" data-tauri-drag-region="">
        <p>{current?.text ?? ""}</p>
      </div>
    </div>
  );
}
