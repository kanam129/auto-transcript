import { create } from "zustand";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, errorMessage } from "../lib/tauri";
import type {
  DegradedEvent,
  DownloadEvent,
  ErrorEvent as AppErrEvent,
  LevelEvent,
  ModelStatus,
  PartialEvent,
  ProfileStatus,
  RecordingState,
  Segment,
  SegmentEvent,
  SessionMeta,
  Settings,
  SilentEvent,
  SourceInfo,
  View,
} from "../lib/types";

export interface LiveSegment {
  id: number;
  start_ms: number;
  end_ms: number;
  text: string;
  lang: string | null;
  edited?: boolean;
}

export interface Notice {
  id: number;
  kind: "warn" | "error";
  title: string;
  detail?: string;
  steps?: string[];
}

interface Store {
  ready: boolean;
  platform: string;
  view: View;
  settings: Settings | null;
  sources: SourceInfo[];
  models: ModelStatus[];
  profiles: ProfileStatus[];
  downloads: Record<string, { downloaded: number; total: number }>;

  recording: boolean;
  sessionId: string | null;
  elapsedMs: number;
  segments: LiveSegment[];
  partial: { text: string; start_ms: number } | null;
  levels: { system: number; mic: number };
  notices: Notice[];

  openSession: { meta: SessionMeta; segments: Segment[] } | null;

  init: () => Promise<void>;
  setView: (v: View) => void;
  toggleRecording: () => Promise<void>;
  refreshSources: () => Promise<void>;
  refreshModels: () => Promise<void>;
  refreshProfiles: () => Promise<void>;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;
  setFontSize: (delta: number | "reset") => Promise<void>;
  toggleAlwaysOnTop: () => Promise<void>;
  openSessionById: (id: string, includeMic?: boolean) => Promise<void>;
  closeSession: () => void;
  notify: (n: Omit<Notice, "id">) => void;
  dismiss: (id: number) => void;
  applyEditedSegment: (id: number, text: string) => void;
}

let noticeSeq = 1;
let unlisteners: UnlistenFn[] = [];
let timer: number | undefined;

/** dBFS → 0..1 for the level meter; -60 dB reads as silence, 0 dB as full. */
export function dbToUnit(db: number): number {
  if (!Number.isFinite(db)) return 0;
  return Math.max(0, Math.min(1, (db + 60) / 60));
}

export const useStore = create<Store>((set, get) => ({
  ready: false,
  platform: "macos",
  view: "live",
  settings: null,
  sources: [],
  models: [],
  profiles: [],
  downloads: {},

  recording: false,
  sessionId: null,
  elapsedMs: 0,
  segments: [],
  partial: null,
  levels: { system: -100, mic: -100 },
  notices: [],
  openSession: null,

  notify: (n) => {
    const id = noticeSeq++;
    set((s) => ({ notices: [...s.notices.filter((x) => x.title !== n.title), { ...n, id }] }));
  },
  dismiss: (id) => set((s) => ({ notices: s.notices.filter((n) => n.id !== id) })),
  setView: (view) => set({ view }),
  closeSession: () => set({ openSession: null, view: "history" }),

  applyEditedSegment: (id, text) =>
    set((s) => ({
      segments: s.segments.map((g) => (g.id === id ? { ...g, text, edited: true } : g)),
      openSession: s.openSession
        ? {
            ...s.openSession,
            segments: s.openSession.segments.map((g) =>
              g.id === id ? { ...g, text, edited: true } : g,
            ),
          }
        : null,
    })),

  init: async () => {
    if (get().ready) return;
    try {
      const [settings, sources, models, profiles, state, platform] = await Promise.all([
        api.getSettings(),
        api.listAudioSources(),
        api.listModels(),
        api.listProfiles(),
        api.getRecordingState(),
        api.appPlatform().catch(() => "macos"),
      ]);
      // Used by CSS for the things that differ per platform, such as the macOS
      // traffic-light gutter that Windows does not have.
      document.documentElement.dataset.os = platform;
      document.documentElement.style.setProperty("--transcript-size", `${settings.font_size}px`);
      set({
        ready: true,
        platform,
        settings,
        sources,
        models,
        profiles,
        recording: state.recording,
        sessionId: state.session_id,
        elapsedMs: state.elapsed_ms,
      });

      // Note: "model not downloaded" and "no audio source" are NOT emitted as
      // notifications here. Both are conditions that can change rather than events that
      // happen once, and a one-shot notification stays on screen after the user has
      // already fixed it. See <Prerequisites />.
    } catch (e) {
      get().notify({ kind: "error", title: "Could not start the app", detail: errorMessage(e) });
      set({ ready: true });
    }

    unlisteners.push(
      await listen<SegmentEvent>("transcript:segment", (ev) => {
        const p = ev.payload;
        set((s) => ({
          segments: [
            ...s.segments,
            {
              id: p.id,
              start_ms: p.start_ms,
              end_ms: p.end_ms,
              text: p.text,
              lang: p.lang,
            },
          ],
          partial: null,
        }));
      }),
      await listen<PartialEvent>("transcript:partial", (ev) => {
        set({ partial: { text: ev.payload.text, start_ms: ev.payload.start_ms } });
      }),
      await listen<LevelEvent>("audio:level", (ev) => {
        set((s) => ({ levels: { ...s.levels, [ev.payload.track]: ev.payload.db } }));
      }),
      await listen<SilentEvent>("audio:silent", (ev) => {
        const { sources, settings } = get();
        const active = sources.find((s) => s.id === settings?.system_source_id);
        const title = `No audio for ${ev.payload.seconds} seconds`;

        // The way out depends on which kind of source is in use, so the guidance should
        // not be generic.
        if (active?.kind === "virtual_loopback") {
          get().notify({
            kind: "warn",
            title,
            detail: `System audio is not being routed to ${active.name}. The simplest fix is to switch the source to your output device directly in Settings — no driver needed. Or set up a Multi-Output Device:`,
            steps: [
              "Open Audio MIDI Setup (⌘Space, type 'Audio MIDI').",
              "Click + at the bottom left → Create Multi-Output Device.",
              `Tick the speakers/headphones you use AND ${active.name}.`,
              "Right-click the Multi-Output Device → Use This Device For Sound Output.",
            ],
          });
        } else {
          get().notify({
            kind: "warn",
            title,
            detail: `Make sure audio is actually playing through ${active?.name ?? "that output device"}. If your meeting plays through a different device, change the source in Settings.`,
          });
        }
      }),
      await listen<AppErrEvent>("audio:error", (ev) => {
        get().notify({ kind: "error", title: "Audio problem", detail: ev.payload.message });
      }),
      await listen<DegradedEvent>("engine:degraded", (ev) => {
        get().notify({
          kind: "warn",
          title: `Switched down to ${ev.payload.to}`,
          detail: `${ev.payload.reason}. Accuracy drops for a while so the transcript can keep up.`,
        });
      }),
      await listen<DownloadEvent>("model:download", (ev) => {
        const p = ev.payload;
        set((s) => ({
          downloads: { ...s.downloads, [p.model_id]: { downloaded: p.downloaded, total: p.total } },
        }));
      }),
      await listen("shortcut:toggle", () => {
        void get().toggleRecording();
      }),
      await listen<RecordingState>("session:state", (ev) => {
        set({
          recording: ev.payload.recording,
          sessionId: ev.payload.session_id,
          elapsedMs: ev.payload.elapsed_ms,
        });
      }),
    );

    timer = window.setInterval(async () => {
      if (!get().recording) return;
      try {
        const st = await api.getRecordingState();
        set({ elapsedMs: st.elapsed_ms, recording: st.recording });
      } catch {
        /* ignored: the clock poll must not spam errors */
      }
    }, 1000);
  },

  refreshSources: async () => {
    try {
      set({ sources: await api.listAudioSources() });
    } catch (e) {
      get().notify({ kind: "error", title: "Could not read audio devices", detail: errorMessage(e) });
    }
  },

  refreshProfiles: async () => {
    try {
      set({ profiles: await api.listProfiles() });
    } catch (e) {
      get().notify({ kind: "error", title: "Could not read model profiles", detail: errorMessage(e) });
    }
  },

  refreshModels: async () => {
    try {
      set({ models: await api.listModels() });
    } catch (e) {
      get().notify({ kind: "error", title: "Could not read the model list", detail: errorMessage(e) });
    }
  },

  toggleRecording: async () => {
    const { recording } = get();
    try {
      if (recording) {
        await api.stopSession();
        set({ recording: false, sessionId: null, partial: null, elapsedMs: 0 });
      } else {
        set({ segments: [], partial: null, notices: [] });
        const st = await api.startSession({});
        set({ recording: true, sessionId: st.session_id, elapsedMs: 0, view: "live" });
      }
    } catch (e) {
      get().notify({
        kind: "error",
        title: recording ? "Could not stop recording" : "Could not start recording",
        detail: errorMessage(e),
      });
      set({ recording: (await api.getRecordingState()).recording });
    }
  },

  saveSettings: async (patch) => {
    const current = get().settings;
    if (!current) return;
    const next = { ...current, ...patch };
    try {
      const saved = await api.setSettings(next);
      set({ settings: saved });
      document.documentElement.style.setProperty("--transcript-size", `${saved.font_size}px`);
    } catch (e) {
      get().notify({ kind: "error", title: "Could not save settings", detail: errorMessage(e) });
    }
  },

  setFontSize: async (delta) => {
    const s = get().settings;
    if (!s) return;
    const next = delta === "reset" ? 18 : Math.max(14, Math.min(36, s.font_size + delta));
    await get().saveSettings({ font_size: next });
  },

  toggleAlwaysOnTop: async () => {
    const s = get().settings;
    if (!s) return;
    const next = !s.always_on_top;
    try {
      await getCurrentWindow().setAlwaysOnTop(next);
      await get().saveSettings({ always_on_top: next });
    } catch (e) {
      get().notify({ kind: "error", title: "Could not toggle always-on-top", detail: errorMessage(e) });
    }
  },

  openSessionById: async (id, includeMic = false) => {
    try {
      const full = await api.getSession(id, includeMic);
      set({ openSession: full, view: "session" });
    } catch (e) {
      get().notify({ kind: "error", title: "Could not open the session", detail: errorMessage(e) });
    }
  },
}));

export function teardown() {
  unlisteners.forEach((u) => u());
  unlisteners = [];
  if (timer) window.clearInterval(timer);
}
