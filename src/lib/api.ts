import { invoke } from "@tauri-apps/api/core";
import brandJson from "../../brand.json";

export const brand = brandJson;

export type Chapter = { start: number; title: string };
export type Video = {
  id: string; share_id: string; title: string; created_at: string; duration: number;
  summary: string | null; chapters: Chapter[]; has_transcript: boolean; public_questions: boolean; edited: boolean;
  voice: Voice;
};
export type Voice = { gain_db: number; auto: boolean };
export type Level = { lufs: number; true_peak: number };
export type VoiceStats = { raw: Level; applied: Level | null; predicted_peak: number; clips: boolean };
export type Cue = [number, number, string];
export type VideoDetail = { video: Video; cues: Cue[]; has_original: boolean };
export type Device = { index: string; name: string };
export type Devices = { screens: Device[]; mics: Device[] };
export type StartOptions = { screen: string; mic: string | null };
export type RecState = { recording: boolean; paused: boolean; elapsed: number };
export type CutOptions = { remove_fillers: boolean; remove_silences: boolean; min_silence: number; pad: number };
export type Range = { start: number; end: number };
export type CutPlan = { keep: Range[]; removed: [Range, string][]; original: number; result: number };
export type Answer = { answer: string; timestamps: number[] };
export type Provider = "cli" | "api";
export type Settings = { provider: Provider; api_key: string | null; voice_default: Voice };
export type AiStatus = { provider: Provider; cli_installed: boolean; cli_version: string | null; api_key_set: boolean; model: string };
export type Tools = { ffmpeg: boolean; whisper: boolean; model: boolean; cloudflared: boolean; library: string };

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
  saveSettings: (provider: Provider, apiKey: string | null, voiceDefault?: Voice) => invoke<void>("save_settings", { provider, apiKey, voiceDefault: voiceDefault ?? null }),
  setVoice: (id: string, voice: Voice) => invoke<Video>("set_voice", { id, voice }),
  voiceStats: (id: string, voice: Voice) => invoke<VoiceStats>("voice_stats", { id, voice }),
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
