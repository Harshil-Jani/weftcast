import { useState } from "react";
import { mmss, type Video } from "../lib/api";

/** Library thumbnail. Hovering plays a muted preview; right-click opens the native context menu. */
export function VideoCard({ video: v, srv, onOpen, onMenu, detail }: {
  video: Video; srv: string; onOpen: () => void; onMenu?: () => void; detail?: string;
}) {
  const [hover, setHover] = useState(false);
  const [ready, setReady] = useState(false);
  const date = new Date(v.created_at).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
  return (
    <button type="button" className="card" onClick={onOpen}
      onContextMenu={onMenu && ((e) => { e.preventDefault(); onMenu(); })}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => { setHover(false); setReady(false); }}
      aria-label={`${v.title}, ${mmss(v.duration)}, ${date}`}>
      <div className="thumb" style={{ backgroundImage: srv ? `url(${srv}/thumb/${v.share_id}.jpg?d=${v.duration})` : undefined }}>
        {hover && srv && (
          <video className={ready ? "on" : ""} src={`${srv}/raw/${v.share_id}.mp4?d=${v.duration}#t=${Math.min(5, v.duration / 4).toFixed(1)}`}
            muted autoPlay loop playsInline onPlaying={() => setReady(true)} />
        )}
        <span className="badge">{mmss(v.duration)}</span>
      </div>
      <div className="meta">
        <b>{v.title}</b>
        <span>{detail ?? [date, v.has_transcript ? "Transcript" : null, v.edited ? "Edited" : null].filter(Boolean).join(" · ")}</span>
      </div>
    </button>
  );
}
