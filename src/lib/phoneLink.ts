import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { base, type PairRole, type PairedPhone } from "./api";

/** A connected phone, as this app's windows see it. */
export type LivePhone = { id: string; role: PairRole; epoch: number; device: string | null; offer: string | null; answered: boolean };
export type PairState = { paired: boolean; connected: boolean; device: string | null; pending: { id: string; role: PairRole } | null; phones: LivePhone[] };

async function post(path: string, body: unknown) {
  await fetch(`${await base()}${path}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
}

export async function pairState(): Promise<PairState> {
  const r = await fetch(`${await base()}/api/pair/state`);
  return r.json();
}

export type PhoneStatus = { connected: boolean; device: string | null; phones: LivePhone[] };

/** Whether a phone is connected right now, and which; polled for the status dots. */
export function usePhoneStatus(ms = 1500): PhoneStatus {
  const [s, set] = useState<PhoneStatus>({ connected: false, device: null, phones: [] });
  useEffect(() => {
    let live = true;
    const tick = () => pairState().then((p) => live && set({ connected: !!p.connected, device: p.device ?? null, phones: p.phones ?? [] })).catch(() => {});
    tick();
    const t = window.setInterval(tick, ms);
    return () => { live = false; clearInterval(t); };
  }, [ms]);
  return s;
}

/** One phone's job (`id`), or the job the next phone gets (no `id`). */
export const setPairRole = (role: PairRole, id?: string) => post("/api/pair/role", { role, id });
export const disconnectPhone = (id: string) => post("/api/pair/disconnect", { id });
export async function pairedPhones(): Promise<PairedPhone[]> {
  const r = await fetch(`${await base()}/api/pair/phones`);
  return (await r.json()).phones ?? [];
}
export const forgetPhone = (id: string) => post("/api/pair/phones/forget", { id });

/** Recording status for the phone's remote tab. */
export const postStatus = (s: { recording: boolean; paused: boolean; elapsed: number; level: number }) => post("/api/pair/status", s).catch(() => {});

const settle = (pc: RTCPeerConnection) => new Promise<void>((done) => {
  if (pc.iceGatheringState === "complete") return done();
  pc.addEventListener("icegatheringstatechange", () => { if (pc.iceGatheringState === "complete") done(); });
  setTimeout(done, 2500);
});

/** The format this web view records best: MP4 (Safari and the Mac web view), else WebM (Chromium). */
const recordingType = () => ["video/mp4;codecs=avc1", "video/mp4", "video/webm;codecs=vp9", "video/webm"].find((t) => MediaRecorder.isTypeSupported(t)) ?? "";

/**
 * Receives a paired phone's camera in this window, for one role. The stream goes to `onStream`
 * (null when it drops). While the screen records, the stream is recorded too, paused with it, and
 * uploaded as its own track once the recording is saved; the start time places it on the timeline.
 */
export function receivePhone(role: PairRole, onStream: (s: MediaStream | null, device: string) => void): () => void {
  let pc: RTCPeerConnection | null = null;
  let answered = "";
  let stream: MediaStream | null = null;
  let device = "";
  let stopped = false;
  let timer = 0;
  let rec: { r: MediaRecorder; chunks: Blob[]; startedAt: number } | null = null;

  const tick = async () => {
    if (stopped) return;
    try {
      const st = await pairState();
      const mine = (st.phones ?? []).find((p) => p.role === role);
      if (mine?.offer && mine.offer !== answered) {
        answered = mine.offer;
        device = mine.device || "Phone";
        pc?.close();
        const conn = new RTCPeerConnection({ iceServers: [] });
        pc = conn;
        conn.ontrack = (e) => { stream = e.streams[0] ?? new MediaStream([e.track]); onStream(stream, device); };
        conn.onconnectionstatechange = () => { if (["failed", "closed"].includes(conn.connectionState)) { stream = null; onStream(null, device); } };
        await conn.setRemoteDescription({ type: "offer", sdp: mine.offer });
        await conn.setLocalDescription(await conn.createAnswer());
        await settle(conn);
        await post("/api/pair/answer", { id: mine.id, sdp: conn.localDescription?.sdp });
      }
      // No phone in this job any more (it left, or moved to another job): drop its picture.
      if (!mine && pc) { pc.close(); pc = null; answered = ""; stream = null; onStream(null, device); }
    } catch { /* the local server is not up yet; try again */ }
    timer = window.setTimeout(tick, 700);
  };
  tick();

  const kind = role === "camera" ? "phone_camera" : "angle";
  const offs = [
    listen<{ id: string; at: number }>("recording-started", () => {
      if (!stream || rec) return;
      const type = recordingType();
      const r = new MediaRecorder(stream, type ? { mimeType: type, videoBitsPerSecond: 6_000_000 } : undefined);
      const chunks: Blob[] = [];
      r.ondataavailable = (e) => { if (e.data.size) chunks.push(e.data); };
      r.start(1000);
      rec = { r, chunks, startedAt: Date.now() };
    }),
    listen<boolean>("recording-paused", (e) => {
      if (!rec) return;
      if (e.payload && rec.r.state === "recording") rec.r.pause();
      if (!e.payload && rec.r.state === "paused") rec.r.resume();
    }),
    listen<string>("recording-saved", (e) => {
      const cur = rec;
      rec = null;
      if (!cur) return;
      cur.r.onstop = async () => {
        const blob = new Blob(cur.chunks, { type: cur.r.mimeType });
        const ext = cur.r.mimeType.includes("mp4") ? "mp4" : "webm";
        const q = new URLSearchParams({ video: e.payload, kind, start: String(cur.startedAt), device, ext });
        await fetch(`${await base()}/api/pair/track?${q}`, { method: "POST", body: blob }).catch(() => {});
      };
      cur.r.stop();
    }),
    listen("recording-discarded", () => { rec?.r.stop(); rec = null; }),
  ];
  return () => { stopped = true; clearTimeout(timer); pc?.close(); offs.forEach((o) => o.then((f) => f())); };
}
