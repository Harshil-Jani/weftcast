import { useEffect, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { api, errText, mmss, type RecState, type StartOptions } from "../lib/api";

/** The floating controls bar: countdown, timer, pause/resume, stop, cancel. */
export default function Hud() {
  const [count, setCount] = useState<number | null>(null);
  const [st, setSt] = useState<RecState>({ recording: false, paused: false, elapsed: 0 });
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    const off = listen<StartOptions>("countdown", async (e) => {
      setErr("");
      for (let n = 3; n > 0; n--) { setCount(n); await new Promise((r) => setTimeout(r, 1000)); }
      setCount(null);
      try { await api.start(e.payload); } catch (x) { setErr(errText(x)); await emit("bubble", { on: false }); }
    });
    timer.current = window.setInterval(() => { api.state().then(setSt); }, 400);
    return () => { off.then((f) => f()); if (timer.current) clearInterval(timer.current); };
  }, []);

  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try { await fn(); } catch (x) { setErr(errText(x)); } finally { setBusy(false); }
  };
  const stop = () => act(async () => { await emit("bubble", { on: false }); await api.stop(); });
  const cancel = () => act(async () => { await emit("bubble", { on: false }); await api.cancel(); });

  if (err) {
    return (
      <div className="hud hud-error" data-tauri-drag-region>
        <span title={err}>{err.split("\n")[0]}</span>
        <button onClick={() => api.privacy("screen")}>Open settings</button>
        <button onClick={() => { setErr(""); api.overlays(false, false); }}>Close</button>
      </div>
    );
  }
  if (count !== null) return <div className="hud countdown" data-tauri-drag-region>Recording in {count}</div>;

  return (
    <div className="hud" data-tauri-drag-region>
      <span className={`rec ${st.paused ? "paused" : ""}`} />
      <span className="time" data-tauri-drag-region>{mmss(st.elapsed)}</span>
      {st.paused
        ? <button disabled={busy} onClick={() => act(api.resume)}>Resume</button>
        : <button disabled={busy || !st.recording} onClick={() => act(api.pause)}>Pause</button>}
      <button className="stop" disabled={busy || !st.recording} onClick={stop}>{busy ? "Saving..." : "Stop"}</button>
      <button className="ghost" disabled={busy} onClick={cancel} title="Discard this recording">✕</button>
    </div>
  );
}
