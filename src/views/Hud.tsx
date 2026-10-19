import { useEffect, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { Menu } from "@tauri-apps/api/menu";
import { ChevronDown, Mic, MicOff, Pause, Play, Smartphone, Trash2, Video, VideoOff, X } from "lucide-react";
import { api, errText, loadPrefs, mmss, savePrefs, type Devices, type RecState, type StartOptions } from "../lib/api";
import { beginRecording } from "../lib/recording";
import { pairState, postStatus, receivePhone } from "../lib/phoneLink";
import { IconButton } from "../components/ui";

/** Live microphone level (0 to 1) while recording, for the phone's remote tab. */
function useMicLevel(on: boolean) {
  const level = useRef(0);
  useEffect(() => {
    if (!on) { level.current = 0; return; }
    let stream: MediaStream | null = null, ctx: AudioContext | null = null, raf = 0, live = true;
    navigator.mediaDevices.getUserMedia({ audio: true, video: false }).then((s) => {
      if (!live) { s.getTracks().forEach((t) => t.stop()); return; }
      stream = s; ctx = new AudioContext();
      const an = ctx.createAnalyser(); an.fftSize = 512;
      ctx.createMediaStreamSource(s).connect(an);
      const buf = new Float32Array(an.fftSize);
      const read = () => { an.getFloatTimeDomainData(buf); level.current = Math.min(1, Math.sqrt(buf.reduce((a, x) => a + x * x, 0) / buf.length) * 4); raf = requestAnimationFrame(read); };
      read();
    }).catch(() => {});
    return () => { live = false; cancelAnimationFrame(raf); stream?.getTracks().forEach((t) => t.stop()); ctx?.close(); };
  }, [on]);
  return level;
}

type Mode = "idle" | "setup" | "countdown" | "recording" | "error";

/** The floating pill: pick a source and start, then a big countdown, then timer, pause, stop and discard. */
export default function Hud() {
  const [mode, setMode] = useState<Mode>("idle");
  const [count, setCount] = useState(3);
  const [st, setSt] = useState<RecState>({ recording: false, paused: false, elapsed: 0 });
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const [devs, setDevs] = useState<Devices | null>(null);
  const saved = loadPrefs();
  const [screen, setScreen] = useState(saved?.screen ?? "");
  const [mic, setMic] = useState<string | null>(saved ? saved.mic : "");
  const [lastMic, setLastMic] = useState(saved?.mic ?? "");
  const [camera, setCamera] = useState(saved?.camera ?? true);
  // A phone paired over Wi-Fi (face cam or second angle), and a phone screen chosen in Connect Phone.
  const [phoneName, setPhoneName] = useState<string | null>(null);
  const phoneScreen = loadPrefs()?.phone ?? null;
  useEffect(() => receivePhone("angle", () => {}), []);
  useEffect(() => {
    const t = window.setInterval(() => { pairState().then((p) => setPhoneName(p.paired && p.connected ? p.device || "Phone" : null)).catch(() => {}); }, 1000);
    return () => clearInterval(t);
  }, []);
  const level = useMicLevel(st.recording && !st.paused);
  useEffect(() => { postStatus({ ...st, level: level.current }); }, [st, level]);
  const connectPhone = async () => { await api.showMain(); await emit("connect-phone"); };
  // The pill widens to fit the phone label.
  const phoneLabel = phoneName ?? phoneScreen?.name ?? null;
  useEffect(() => {
    if (mode === "setup") api.hudLayout(phoneLabel ? "setup-phone" : "setup");
    if (mode === "recording") api.hudLayout(phoneName ? "recording-phone" : "recording");
  }, [mode, phoneLabel, phoneName]);

  const countdown = async (opts: StartOptions) => {
    setErr(""); setMode("countdown");
    await api.hudLayout("countdown");
    for (let n = 3; n > 0; n--) { setCount(n); await new Promise((r) => setTimeout(r, 1000)); }
    await api.hudLayout("recording");
    setMode("recording");
    try { await api.start(opts); } catch (x) { fail(x); await emit("bubble", { on: false }); }
  };
  const fail = async (x: unknown) => { setErr(errText(x)); setMode("error"); await api.hudLayout("error"); };

  useEffect(() => {
    const offs = [
      listen("hud-setup", async () => {
        setErr(""); setMode("setup");
        try {
          const d = await api.devices();
          setDevs(d);
          setScreen((s) => (d.screens.some((x) => x.index === s) ? s : d.screens[0]?.index ?? ""));
          setMic((m) => (m === null || d.mics.some((x) => x.index === m) ? m : d.mics[0]?.index ?? null));
          setLastMic((m) => (d.mics.some((x) => x.index === m) ? m : d.mics[0]?.index ?? ""));
        } catch (x) { fail(x); }
      }),
      listen<StartOptions>("countdown", (e) => countdown(e.payload)),
      // Buttons on the phone's Remote tab.
      listen<string>("remote", async (e) => {
        const s = await api.state();
        const p = loadPrefs();
        if (e.payload === "start" && !s.recording) { if (p) await beginRecording(p); }
        else if (e.payload === "pause" && s.recording && !s.paused) await api.pause();
        else if (e.payload === "resume" && s.paused) await api.resume();
        else if (e.payload === "stop" && s.recording) { await emit("bubble", { on: false }); await api.stop(); setMode("idle"); }
        else if (e.payload === "discard" && s.recording) { await emit("bubble", { on: false }); await api.cancel(); setMode("idle"); }
      }),
    ];
    const timer = window.setInterval(() => { api.state().then((s) => { setSt(s); if (s.recording) setMode((m) => (m === "idle" ? "recording" : m)); }); }, 400);
    return () => { offs.forEach((o) => o.then((f) => f())); clearInterval(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try { await fn(); } catch (x) { fail(x); } finally { setBusy(false); }
  };
  const close = async () => { setMode("idle"); await emit("bubble", { on: false }); await api.overlays(false, false); };
  const stop = () => act(async () => { await emit("bubble", { on: false }); await api.stop(); setMode("idle"); });
  const discard = () => act(async () => { await emit("bubble", { on: false }); await api.cancel(); setMode("idle"); });
  const toggleCamera = async () => {
    const on = !camera; setCamera(on);
    await api.overlays(true, on); await emit("bubble", { on });
  };
  const micMenu = async () => {
    const items = (devs?.mics ?? []).map((m) => ({ id: `mic-${m.index}`, text: m.name, checked: mic === m.index, action: () => { setMic(m.index); setLastMic(m.index); } }));
    const menu = await Menu.new({ items: [...items, { id: "mic-none", text: "No Microphone", checked: mic === null, action: () => setMic(null) }] });
    await menu.popup();
  };
  const record = async () => {
    const p = { ...loadPrefs(), screen, mic, camera, phone: phoneScreen };
    savePrefs(p);
    await api.hideMain();
    countdown({ screen, mic, phone: phoneScreen });
  };

  if (mode === "countdown") {
    return <div className="countdown" role="timer" aria-live="assertive"><b key={count}>{count}</b><span>Recording starts</span></div>;
  }
  if (mode === "error") {
    return (
      <div className="hud" data-tauri-drag-region role="alert">
        <span className="msg" title={err} data-tauri-drag-region>{err.split("\n")[0]}</span>
        <button type="button" className="btn" onClick={() => api.privacy("screen")}>Open Settings</button>
        <IconButton icon={X} label="Close" onClick={() => { setErr(""); close(); }} />
      </div>
    );
  }
  if (mode === "recording") {
    return (
      <div className="hud" data-tauri-drag-region role="toolbar" aria-label="Recording controls">
        <span className={`dot ${st.paused ? "paused" : ""}`} aria-hidden />
        <span className="time" data-tauri-drag-region aria-label={`Recorded ${mmss(st.elapsed)}`}>{mmss(st.elapsed)}</span>
        {phoneName && <span className="phone-chip" title={`Phone connected: ${phoneName}`}><i />Phone connected: {phoneName}</span>}
        <span className="spacer" data-tauri-drag-region />
        <IconButton icon={Smartphone} label="Connect Phone" onClick={connectPhone} />
        {st.paused
          ? <IconButton icon={Play} label="Resume" disabled={busy} onClick={() => act(api.resume)} />
          : <IconButton icon={Pause} label="Pause" disabled={busy || !st.recording} onClick={() => act(api.pause)} />}
        <IconButton icon={Trash2} label="Discard recording" disabled={busy} onClick={discard} />
        <button type="button" className="stop" aria-label="Stop and save" title="Stop and save (Option Shift R)" disabled={busy || !st.recording} onClick={stop}><i /></button>
      </div>
    );
  }
  // Setup pill.
  return (
    <div className="hud" data-tauri-drag-region role="toolbar" aria-label="New recording">
      <IconButton icon={X} label="Cancel" onClick={close} />
      <span className="divider" />
      <select aria-label="Screen" value={screen} onChange={(e) => setScreen(e.target.value)} disabled={!devs}>
        {!devs && <option>Finding screens...</option>}
        {devs?.screens.map((d) => <option key={d.index} value={d.index}>{d.name}</option>)}
      </select>
      <span className="split" role="group" aria-label="Microphone">
        <IconButton icon={mic === null ? MicOff : Mic} label={mic === null ? "Microphone off" : "Microphone on"} pressed={mic !== null} onClick={() => setMic(mic === null ? lastMic || devs?.mics[0]?.index || null : null)} />
        <IconButton icon={ChevronDown} small label="Choose microphone" disabled={!devs?.mics.length} onClick={micMenu} />
      </span>
      <IconButton icon={camera ? Video : VideoOff} label={camera ? "Camera on" : "Camera off"} pressed={camera} onClick={toggleCamera} />
      <IconButton icon={Smartphone} label="Connect Phone" pressed={!!(phoneName || phoneScreen)} onClick={connectPhone} />
      {(phoneName || phoneScreen) && (
        <span className="phone-chip" title={phoneName ? `Phone connected: ${phoneName}` : `Phone screen: ${phoneScreen?.name}`}>
          <i />{phoneName ? `Phone connected: ${phoneName}` : phoneScreen?.name}
        </span>
      )}
      <span className="spacer" data-tauri-drag-region />
      <button type="button" className="rec-btn" disabled={!screen} onClick={record}><i aria-hidden />Record</button>
    </div>
  );
}
