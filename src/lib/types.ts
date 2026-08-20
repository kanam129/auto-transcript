export type Track = "system" | "mic";
export type LangMode = "auto" | "en" | "id";
export type View = "live" | "history" | "session" | "settings";

/** system_output = disadap langsung tanpa driver; virtual_loopback = butuh BlackHole dsb. */
export type SourceKind = "mic" | "system_output" | "virtual_loopback";

export interface SourceInfo {
  id: string;
  name: string;
  kind: SourceKind;
  sample_rate: number;
  channels: number;
  is_default: boolean;
}

export interface LlmSettings {
  base_url: string;
  model: string;
  timeout_secs: number;
}

export interface Settings {
  system_source_id: string | null;
  mic_source_id: string | null;
  mic_transcribe: boolean;
  model_id: string;
  partial_model_id: string;
  partials_enabled: boolean;
  lang_mode: LangMode;
  font_size: number;
  always_on_top: boolean;
  keep_audio_days: number | null;
  keep_audio: boolean;
  vocabulary: string;
  summary_enabled: boolean;
  llm: LlmSettings;
  onboarded: boolean;
  version: number;
}

export interface SessionMeta {
  id: string;
  title: string;
  started_at: number;
  ended_at: number | null;
  duration_ms: number | null;
  model_used: string | null;
  source_name: string | null;
  lang_mode: string;
  system_wav_path: string | null;
  mic_wav_path: string | null;
  segment_count: number;
  word_count: number;
  has_summary: boolean;
  snippet: string | null;
}

export interface Segment {
  id: number;
  session_id: string;
  track: Track;
  start_ms: number;
  end_ms: number;
  text: string;
  lang: string | null;
  confidence: number | null;
  edited: boolean;
}

export interface RecordingState {
  recording: boolean;
  session_id: string | null;
  title: string | null;
  elapsed_ms: number;
  model_id: string | null;
  source_name: string | null;
  queue_depth: number;
}

export interface ModelStatus {
  id: string;
  name: string;
  filename: string;
  url: string;
  size_bytes: number;
  sha256: string;
  note: string;
  downloaded: boolean;
  path: string;
}

export interface ProfileStatus {
  id: string;
  name: string;
  model_id: string;
  partial_model_id: string;
  note: string;
  recommended: boolean;
  download_bytes: number;
  total_bytes: number;
  ready: boolean;
}

export interface DataPaths {
  data_dir: string;
  models_dir: string;
  recordings_dir: string;
  database: string;
  log_dir: string;
}

export interface AppErrorPayload {
  code: string;
  message: string;
}

// --- payload event ---
export interface SegmentEvent {
  session_id: string;
  id: number;
  start_ms: number;
  end_ms: number;
  text: string;
  lang: string | null;
  confidence: number | null;
}
export interface PartialEvent {
  session_id: string;
  start_ms: number;
  text: string;
}
export interface LevelEvent {
  track: Track;
  db: number;
}
export interface SilentEvent {
  track: Track;
  seconds: number;
}
export interface ErrorEvent {
  code: string;
  message: string;
}
export interface DownloadEvent {
  model_id: string;
  downloaded: number;
  total: number;
}
export interface DegradedEvent {
  from: string;
  to: string;
  reason: string;
}
export interface MicProgressEvent {
  session_id: string;
  done: number;
  total: number;
}
