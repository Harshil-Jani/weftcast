import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/** Round camera window. The camera only runs while the bubble is on, so the light goes off after recording. */
export default function Bubble() {
  const video = useRef<HTMLVideoElement>(null);
  const stream = useRef<MediaStream | null>(null);
  const [err, setErr] = useState("");

  useEffect(() => {
    const stopCam = () => { stream.current?.getTracks().forEach((t) => t.stop()); stream.current = null; };
    const off = listen<{ on: boolean }>("bubble", async (e) => {
      if (!e.payload.on) return stopCam();
      if (stream.current) return;
      try {
        stream.current = await navigator.mediaDevices.getUserMedia({ video: { width: 640, height: 640 }, audio: false });
        if (video.current) video.current.srcObject = stream.current;
        setErr("");
      } catch {
        setErr("Allow camera access in System Settings");
      }
    });
    return () => { off.then((f) => f()); stopCam(); };
  }, []);

  return (
    <div className="bubble" data-tauri-drag-region>
      {err ? <span className="bubble-err">{err}</span> : <video ref={video} autoPlay muted playsInline />}
    </div>
  );
}
