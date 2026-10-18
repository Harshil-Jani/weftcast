import { useCallback, useEffect, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { api, base, brand, errText, mmss, type Devices, type StartOptions, type Video } from "../lib/api";
import Player from "./Player";
import Settings from "./Settings";

type Tab = "library" | "settings";
const PREFS = "rec-prefs";

type Prefs = StartOptions & { camera: boolean };
function loadPrefs(): Prefs | null {
  try { return JSON.parse(localStorage.getItem(PREFS) || "null"); } catch { return null; }
}

/** Starts the countdown in the controls window and gets the library out of the way. */
export async function beginRecording(p: Prefs) {
  localStorage.setItem(PREFS, JSON.stringify(p));
  await api.overlays(true, p.camera);
  await emit("bubble", { on: p.camera });
  await emit("countdown", { screen: p.screen, mic: p.mic });
  await api.hideMain();
}

export default function App() {
  const [tab, setTab] = useState<Tab>("library");
  const [videos, setVideos] = useState<Video[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [newRec, setNewRec] = useState(false);
  const [srv, setSrv] = useState("");

  const refresh = useCallback(() => { api.list().then(setVideos); }, []);
  useEffect(() => {
    refresh();
    base().then(setSrv);
    const offs = [
      listen<string>("recording-saved", (e) => { refresh(); setOpen(e.payload); setTab("library"); }),
      // Global shortcut and tray: stop if recording, otherwise start with the last settings.
      listen("toggle-recording", async () => {
        const s = await api.state();
        if (s.recording) { await emit("bubble", { on: false }); await api.stop(); return; }
        const p = loadPrefs();
        if (p) await beginRecording(p); else setNewRec(true);
      }),
    ];
    return () => { offs.forEach((o) => o.then((f) => f())); };
  }, [refresh]);

  return (
    <div className="shell">
      <aside className="side">
        <div className="logo"><span className="dot" />{brand.name}</div>
        <button className="primary big" onClick={() => setNewRec(true)}>● New recording</button>
        <nav>
          <button className={tab === "library" && !open ? "on" : ""} onClick={() => { setTab("library"); setOpen(null); }}>Library</button>
          <button className={tab === "settings" ? "on" : ""} onClick={() => { setTab("settings"); setOpen(null); }}>Settings</button>
        </nav>
        <p className="hint">Alt+Shift+R starts and stops a recording from anywhere.</p>
      </aside>
      <main className="content">
        {tab === "settings" ? <Settings /> : open ? (
          <Player id={open} onBack={() => { setOpen(null); refresh(); }} onDeleted={() => { setOpen(null); refresh(); }} />
        ) : (
          <Library videos={videos} srv={srv} onOpen={setOpen} onNew={() => setNewRec(true)} />
        )}
      </main>
      {newRec && <NewRecording onClose={() => setNewRec(false)} />}
    </div>
  );
}

function Library({ videos, srv, onOpen, onNew }: { videos: Video[]; srv: string; onOpen: (id: string) => void; onNew: () => void }) {
  if (!videos.length) {
    return (
      <div className="empty">
        <h1>No recordings yet</h1>
        <p>{brand.tagline}</p>
        <button className="primary" onClick={onNew}>Record your first video</button>
      </div>
    );
  }
  return (
    <>
      <h1 className="page-title">Library</h1>
      <div className="grid">
        {videos.map((v) => (
          <button key={v.id} className="card video-card" onClick={() => onOpen(v.id)}>
            <div className="thumb" style={{ backgroundImage: srv ? `url(${srv}/thumb/${v.share_id}.jpg?d=${v.duration})` : undefined }}>
              <span className="dur">{mmss(v.duration)}</span>
            </div>
            <div className="meta">
              <b>{v.title}</b>
              <small>{new Date(v.created_at).toLocaleString()}{v.has_transcript ? " · transcript" : ""}</small>
            </div>
          </button>
        ))}
      </div>
    </>
  );
}

function NewRecording({ onClose }: { onClose: () => void }) {
  const saved = loadPrefs();
  const [devs, setDevs] = useState<Devices | null>(null);
  const [screen, setScreen] = useState(saved?.screen ?? "");
  const [mic, setMic] = useState<string>(saved?.mic ?? "");
  const [camera, setCamera] = useState(saved?.camera ?? true);
  const [err, setErr] = useState("");

  useEffect(() => {
    api.devices().then((d) => {
      setDevs(d);
      setScreen((s) => (d.screens.some((x) => x.index === s) ? s : d.screens[0]?.index ?? ""));
      setMic((m) => (m === "none" || d.mics.some((x) => x.index === m) ? m : d.mics[0]?.index ?? "none"));
    }).catch((e) => setErr(errText(e)));
  }, []);

  const go = async () => {
    try {
      await beginRecording({ screen, mic: mic === "none" ? null : mic, camera });
      onClose();
    } catch (e) { setErr(errText(e)); }
  };

  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>New recording</h2>
        {!devs && !err && <p className="muted">Looking for screens and microphones...</p>}
        {devs && (
          <>
            <label>Screen
              <select value={screen} onChange={(e) => setScreen(e.target.value)}>
                {devs.screens.map((d) => <option key={d.index} value={d.index}>{d.name.replace("Capture screen", "Display")}</option>)}
              </select>
            </label>
            <label>Microphone
              <select value={mic} onChange={(e) => setMic(e.target.value)}>
                {devs.mics.map((d) => <option key={d.index} value={d.index}>{d.name}</option>)}
                <option value="none">No audio</option>
              </select>
            </label>
            <label className="check"><input type="checkbox" checked={camera} onChange={(e) => setCamera(e.target.checked)} /> Show my camera in a bubble</label>
            <p className="muted small">Recording starts after a 3 second countdown. Stop it from the controls bar, the menu bar icon, or Alt+Shift+R.</p>
          </>
        )}
        {err && <p className="error">{err}</p>}
        <div className="row end">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" disabled={!screen} onClick={go}>Start recording</button>
        </div>
      </div>
    </div>
  );
}
