import { invoke } from "@tauri-apps/api/core";
import brandJson from "../../brand.json";

export const brand = brandJson;

export type Chapter = { start: number; title: string };
export type Video = {
  id: string; share_id: string; title: string; created_at: string; duration: number;
  summary: string | null; chapters: Chapter[]; has_transcript: boolean; public_questions: boolean; edited: boolean;
  voice: Voice; notes: Note[]; caption_langs: string[]; source_url: string | null; bubble: [number, number, number, number] | null;
};
export type Note = { time: number; text: string };
export type Clip = { id: string; start: number; end: number; title: string; reason: string; score: number; score_reason: string; caption: string; selected: boolean };
export type Limits = { count: number; min: number; max: number; allow_overlap: boolean };
export type Framing = "center" | "blur_fit" | "split";
export type ShortsFile = { limits: Limits | null; clips: Clip[]; rendered: string[] };
export type Rendered = { clip: string; file: string; captions: string };
export type EditPlan = { ops: Record<string, unknown>[]; explanation: string };
export type ShortPlan = { ranges: Range[]; length: number; explanation: string };
export type Voice = { gain_db: number; auto: boolean };
export type Level = { lufs: number; true_peak: number };
export type VoiceStats = { raw: Level; applied: Level | null; predicted_peak: number; clips: boolean };
export type Cue = [number, number, string];
export type VideoDetail = { video: Video; cues: Cue[]; has_original: boolean; can_undo: boolean };
export type Device = { index: string; name: string };
export type Devices = { screens: Device[]; mics: Device[] };
export type StartOptions = { screen: string; mic: string | null };
export type RecState = { recording: boolean; paused: boolean; elapsed: number };
export type CutOptions = { remove_fillers: boolean; remove_silences: boolean; min_silence: number; pad: number };
export type Range = { start: number; end: number };
export type CutPlan = { keep: Range[]; removed: [Range, string][]; original: number; result: number };
export type Answer = { answer: string; timestamps: number[] };
export type Provider = "cli" | "api";
export type WhisperModel = "base_en" | "small_en";
export type Settings = { provider: Provider; api_key: string | null; voice_default: Voice; whisper_model: WhisperModel; onboarded: boolean };
export type AiStatus = { provider: Provider; cli_installed: boolean; cli_version: string | null; cli_logged_in: boolean | null; api_key_set: boolean; model: string };
export const aiReady = (s: AiStatus | null) => !!s && (s.provider === "api" ? s.api_key_set : !!s.cli_installed && s.cli_logged_in !== false);
export type Tools = { ffmpeg: boolean; captions_burn: boolean; model: boolean; model_name: WhisperModel; model_mb: number; cloudflared: boolean; ytdlp: boolean; library: string };

export const api = {
  port: () => invoke<number>("server_port"),
  list: () => invoke<Video[]>("list_videos"),
  get: (id: string) => invoke<VideoDetail>("get_video", { id }),
  rename: (id: string, title: string) => invoke<Video>("rename_video", { id, title }),
  remove: (id: string) => invoke<void>("delete_video", { id }),
  reveal: (id: string) => invoke<void>("reveal_video", { id }),
  devices: () => invoke<Devices>("devices"),
  start: (opts: StartOptions) => invoke<string>("start_recording", { opts }),
  pause: () => invoke<void>("pause_recording"),
  resume: () => invoke<void>("resume_recording"),
  state: () => invoke<RecState>("recording_state"),
  stop: () => invoke<Video>("stop_recording"),
  cancel: () => invoke<void>("cancel_recording"),
  overlays: (hud: boolean, bubble: boolean) => invoke<void>("show_overlays", { hud, bubble }),
  hideMain: () => invoke<void>("hide_main"),
  transcribe: (id: string) => invoke<VideoDetail>("transcribe_video", { id }),
  insights: (id: string) => invoke<Video>("generate_insights", { id }),
  ask: (id: string, question: string) => invoke<Answer>("ask_video", { id, question }),
  planCuts: (id: string, opts: CutOptions) => invoke<CutPlan>("plan_cuts", { id, opts }),
  applyCuts: (id: string, opts: CutOptions) => invoke<Video>("apply_cuts", { id, opts }),
  trim: (id: string, start: number, end: number) => invoke<Video>("trim_video", { id, start, end }),
  restore: (id: string) => invoke<Video>("restore_original", { id }),
  exportVideo: (id: string, kind: "mp4" | "captioned" | "srt" | "vtt") => invoke<{ path: string; note: string | null }>("export_video", { id, kind }),
  setPublicQuestions: (id: string, on: boolean) => invoke<Video>("set_public_questions", { id, on }),
  shareStart: () => invoke<string>("share_start"),
  shareStop: () => invoke<void>("share_stop"),
  shareStatus: () => invoke<string | null>("share_status"),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (provider: Provider, apiKey: string | null, voiceDefault?: Voice, extra?: { whisperModel?: WhisperModel; onboarded?: boolean }) =>
    invoke<void>("save_settings", { provider, apiKey, voiceDefault: voiceDefault ?? null, whisperModel: extra?.whisperModel ?? null, onboarded: extra?.onboarded ?? null }),
  forgetKey: () => invoke<void>("forget_api_key"),
  installModel: (path: string, model: WhisperModel) => invoke<void>("install_model_file", { path, model }),
  updateYoutube: () => invoke<string>("update_youtube_importer"),
  setVoice: (id: string, voice: Voice) => invoke<Video>("set_voice", { id, voice }),
  voiceStats: (id: string, voice: Voice) => invoke<VoiceStats>("voice_stats", { id, voice }),
  planEdit: (id: string, prompt: string) => invoke<{ plan: EditPlan; lines: string[] }>("plan_edit", { id, prompt }),
  applyPlan: (id: string, plan: EditPlan) => invoke<Video>("apply_plan", { id, plan }),
  undoEdit: (id: string) => invoke<Video>("undo_edit", { id }),
  planShorter: (id: string, target: number) => invoke<ShortPlan>("plan_shorter", { id, target }),
  renderShorter: (id: string, ranges: Range[]) => invoke<Video>("render_shorter", { id, ranges }),
  translate: (id: string, lang: string) => invoke<Video>("translate_captions", { id, lang }),
  languages: () => invoke<[string, string][]>("caption_languages"),
  importFile: (path: string) => invoke<Video>("import_file", { path }),
  importYoutube: (url: string) => invoke<Video>("import_youtube", { url }),
  shorts: (id: string) => invoke<ShortsFile>("get_shorts", { id }),
  suggestShorts: (id: string, limits: Limits) => invoke<{ clips: Clip[]; rejected: string[] }>("suggest_shorts", { id, limits }),
  rerollShort: (id: string, clipId: string) => invoke<ShortsFile>("reroll_short", { id, clipId }),
  updateShort: (id: string, clip: Clip) => invoke<ShortsFile>("update_short", { id, clip }),
  nudgeShort: (id: string, clipId: string, edge: "start" | "end", delta: number, toSentence: boolean) => invoke<ShortsFile>("nudge_short", { id, clipId, edge, delta, toSentence }),
  renderShorts: (id: string, clipIds: string[], framing: Framing) => invoke<{ files: Rendered[]; note: string | null }>("render_shorts", { id, clipIds, framing }),
  reveal_path: (path: string) => invoke<void>("reveal_path", { path }),
  voicePreview: (id: string, voice: Voice, start: number) => invoke<void>("voice_preview", { id, voice, start }),
  aiStatus: () => invoke<AiStatus>("ai_status"),
  testClaude: () => invoke<string>("test_claude"),
  tools: () => invoke<Tools>("tools_status"),
  privacy: (pane: "screen" | "camera" | "microphone") => invoke<void>("open_privacy_settings", { pane }),
};

export const mmss = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
export const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

let portCache: number | null = null;
export async function base() {
  portCache ??= await api.port();
  return `http://127.0.0.1:${portCache}`;
}
