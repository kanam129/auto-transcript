import { invoke } from "@tauri-apps/api/core";
import type {
  DataPaths,
  ModelStatus,
  ProfileStatus,
  RecordingState,
  Segment,
  SessionMeta,
  Settings,
  SourceInfo,
} from "./types";

/** Every error from Rust arrives as `{ code, message }`. */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}

export const api = {
  listAudioSources: () => invoke<SourceInfo[]>("list_audio_sources"),
  appPlatform: () => invoke<string>("app_platform"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),

  startSession: (args: {
    title?: string | null;
    systemSourceId?: string | null;
    micSourceId?: string | null;
  }) =>
    invoke<RecordingState>("start_session", {
      title: args.title ?? null,
      systemSourceId: args.systemSourceId ?? null,
      micSourceId: args.micSourceId ?? null,
    }),
  stopSession: () => invoke<SessionMeta>("stop_session"),
  getRecordingState: () => invoke<RecordingState>("get_recording_state"),

  listSessions: (q?: string, limit = 50, offset = 0) =>
    invoke<SessionMeta[]>("list_sessions", { q: q || null, limit, offset }),
  getSession: (id: string, includeMic = false) =>
    invoke<{ meta: SessionMeta; segments: Segment[] }>("get_session", { id, includeMic }),
  renameSession: (id: string, title: string) => invoke<void>("rename_session", { id, title }),
  editSegment: (id: number, text: string) => invoke<void>("edit_segment", { id, text }),
  deleteSession: (id: string, deleteAudio: boolean) =>
    invoke<void>("delete_session", { id, deleteAudio }),
  exportSession: (id: string, format: string, path: string, includeMic: boolean) =>
    invoke<string>("export_session", { id, format, path, includeMic }),
  transcribeMic: (id: string) => invoke<void>("transcribe_mic", { id }),

  diskUsage: () => invoke<{ bytes: number; sessions: number }>("disk_usage"),
  cleanupOldAudio: () => invoke<number>("cleanup_old_audio"),
  revealInFinder: (path: string) => invoke<void>("reveal_in_finder", { path }),

  listModels: () => invoke<ModelStatus[]>("list_models"),
  listProfiles: () => invoke<ProfileStatus[]>("list_profiles"),
  applyProfile: (profileId: string) => invoke<Settings>("apply_profile", { profileId }),
  dataPaths: () => invoke<DataPaths>("data_paths"),
  downloadModel: (modelId: string) => invoke<string>("download_model", { modelId }),
  deleteModel: (modelId: string) => invoke<void>("delete_model", { modelId }),

  setLlmApiKey: (key: string) => invoke<void>("set_llm_api_key", { key }),
  hasLlmApiKey: () => invoke<boolean>("has_llm_api_key"),
  getSummary: (id: string) => invoke<string | null>("get_summary", { id }),
  summarizeSession: (id: string) => invoke<string>("summarize_session", { id }),
};

export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 10 ? 0 : 1)} ${units[i]}`;
}

export function formatDate(unixMs: number): string {
  return new Date(unixMs).toLocaleString("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
