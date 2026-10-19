import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, loadPrefs } from "../lib/api";
import { receivePhone } from "../lib/phoneLink";
import { Segmented } from "../components/ui";

const SIZES = { S: 140, M: 200, L: 280 } as const;
type Size = keyof typeof SIZES;

/** Round camera window. The camera only runs while the bubble is on, so the light goes off after recording. */
export default function Bubble() {
  const video = useRef<HTMLVideoElement>(null);
  const stream = useRef<MediaStream | null>(null);
  const [err, setErr] = useState("");
  const [size, setSize] = useState<Size>(() => { try { return (localStorage.getItem("bubble-size") as Size) || "M"; } catch { return "M"; } });

  const [recording, setRecording] = useState(false);
  useEffect(() => { api.bubbleSize(SIZES[size]); }, [size]);
  // The bubble is part of the recording, so the size control hides while recording.
  useEffect(() => {
    const t = window.setInterval(() => api.state().then((s) => setRecording(s.recording)), 1000);
    return () => clearInterval(t);
  }, []);

  // A phone paired as the face cam (Wi-Fi) takes over from the Mac's camera while it is connected.
  const [phone, setPhone] = useState<MediaStream | null>(null);
  const phoneRef = useRef<MediaStream | null>(null);
  useEffect(() => receivePhone("camera", (s) => { phoneRef.current = s; setPhone(s); }), []);

  useEffect(() => {
    const stopCam = () => { stream.current?.getTracks().forEach((t) => t.stop()); stream.current = null; };
    const off = listen<{ on: boolean }>("bubble", async (e) => {
      if (!e.payload.on) return stopCam();
      if (stream.current) return;
      try {
        // The chosen camera, for example an iPhone used as a Continuity Camera.
        const id = loadPrefs()?.cameraId;
        stream.current = await navigator.mediaDevices.getUserMedia({ video: { width: 640, height: 640, ...(id ? { deviceId: { exact: id } } : {}) }, audio: false });
        if (video.current && !phoneRef.current) video.current.srcObject = stream.current;
        setErr("");
      } catch {
        setErr("Allow camera access in System Settings");
      }
    });
    return () => { off.then((f) => f()); stopCam(); };
  }, []);
  // Phone connected: show it. Phone gone: back to the Mac's camera, if it is running.
  useEffect(() => {
    if (video.current) video.current.srcObject = phone ?? stream.current;
  }, [phone]);

  const pick = (s: Size) => { setSize(s); try { localStorage.setItem("bubble-size", s); } catch { /* not persisted */ } };
  return (
    <div className="bubble" data-tauri-drag-region>
      {err ? <span className="err">{err}</span> : <video ref={video} autoPlay muted playsInline />}
      {!recording && <div className="sizes"><Segmented label="Camera size" value={size} onChange={pick} options={(["S", "M", "L"] as Size[]).map((s) => ({ value: s, label: s }))} /></div>}
    </div>
  );
}
