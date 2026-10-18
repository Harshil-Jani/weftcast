import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, base, errText, mmss, type Clip, type Framing, type Limits, type ShortsFile, type Video, type Rendered } from "../lib/api";

const DEFAULT_LIMITS: Limits = { count: 5, min: 15, max: 60, allow_overlap: false };

/** Turn one long video into vertical shorts: import, transcribe, let Claude pick clips, review, render. */
export default function Shorts({ videos, onImported }: { videos: Video[]; onImported: () => void }) {
  const [id, setId] = useState<string | null>(null);
  const [path, setPath] = useState("");
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState("");
  const [msg, setMsg] = useState<{ text: string; error?: boolean } | null>(null);

  const run = async (label: string, fn: () => Promise<unknown>) => {
    setBusy(label); setMsg(null);
    try { await fn(); } catch (e) { setMsg({ text: errText(e), error: true }); } finally { setBusy(""); }
  };
  const video = videos.find((v) => v.id === id);

  if (video) return <ShortsEditor video={video} onBack={() => setId(null)} />;
  return (
    <div className="shorts">
      <h1 className="page-title">Shorts</h1>
      <p className="muted">Turn a long video into several vertical shorts. Claude picks moments that stand on their own; you review them before anything renders.</p>
      <div className="card panel">
        <h3>Import</h3>
        <div className="row"><input value={path} onChange={(e) => setPath(e.target.value)} placeholder="/path/to/video.mp4" />
          <button disabled={!path.trim() || !!busy} onClick={() => run("Importing", async () => { const v = await api.importFile(path.trim()); setPath(""); onImported(); setId(v.id); })}>Import file</button></div>
        <div className="row"><input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://www.youtube.com/watch?v=..." />
          <button disabled={!url.trim() || !!busy} onClick={() => run("Downloading from YouTube", async () => { const v = await api.importYoutube(url.trim()); setUrl(""); onImported(); setId(v.id); })}>Import from YouTube</button></div>
        <p className="warn small">Only import videos you own or have the rights to. Downloading other people's YouTube videos breaks YouTube's terms.</p>
        {busy && <p className="muted">{busy}...</p>}
        {msg && <p className={msg.error ? "error" : "ok"}>{msg.text}</p>}
      </div>
      <h3 className="section">Or pick a video from your library</h3>
      <div className="grid">
        {videos.map((v) => (
          <button key={v.id} className="card video-card" onClick={() => setId(v.id)}>
            <div className="meta"><b>{v.title}</b><small>{mmss(v.duration)}{v.has_transcript ? " · transcript" : ""}{v.source_url ? " · imported" : ""}</small></div>
          </button>
        ))}
      </div>
    </div>
  );
}

function ShortsEditor({ video, onBack }: { video: Video; onBack: () => void }) {
  const [file, setFile] = useState<ShortsFile | null>(null);
  const [limits, setLimits] = useState<Limits>(DEFAULT_LIMITS);
  const [framing, setFraming] = useState<Framing>(video.bubble ? "split" : "blur_fit");
  const [hasTranscript, setHasTranscript] = useState(video.has_transcript);
  const [busy, setBusy] = useState("");
  const [progress, setProgress] = useState("");
  const [msg, setMsg] = useState<{ text: string; error?: boolean } | null>(null);
  const [rendered, setRendered] = useState<Rendered[]>([]);
  const [srv, setSrv] = useState("");

  useEffect(() => {
    api.shorts(video.id).then((f) => { setFile(f); if (f.limits) setLimits(f.limits); });
    base().then(setSrv);
    const off = listen<{ id: string; stage: string; done: number; total: number }>("progress", (e) => {
      if (e.payload.id !== video.id) return;
      const { stage, done, total } = e.payload;
      setProgress(stage === "done" ? "" : total ? `${stage} ${Math.round((done / total) * 100)}%` : `${stage}...`);
    });
    return () => { off.then((f) => f()); };
  }, [video.id]);

  const run = async (label: string, fn: () => Promise<unknown>, done?: string) => {
    setBusy(label); setMsg(null);
    try { await fn(); if (done) setMsg({ text: done }); } catch (e) { setMsg({ text: errText(e), error: true }); } finally { setBusy(""); setProgress(""); }
  };
  const clips = file?.clips ?? [];
  const selected = clips.filter((c) => c.selected);
  const save = (c: Clip) => api.updateShort(video.id, c).then(setFile);

  return (
    <div className="shorts">
      <button className="ghost" onClick={onBack}>← Shorts</button>
      <h1 className="page-title">{video.title}</h1>
      {video.source_url && <p className="muted small">Imported from {video.source_url}</p>}
      <div className="card panel">
        <div className="row wrap">
          {!hasTranscript && <button className="primary" disabled={!!busy} onClick={() => run("Transcribing", async () => { await api.transcribe(video.id); setHasTranscript(true); }, "Transcript ready.")}>Transcribe first</button>}
          <label className="inline">Clips <input className="num" type="number" min={1} max={20} value={limits.count} onChange={(e) => setLimits({ ...limits, count: Number(e.target.value) })} /></label>
          <label className="inline">Length <input className="num" type="number" min={5} value={limits.min} onChange={(e) => setLimits({ ...limits, min: Number(e.target.value) })} /> to <input className="num" type="number" min={5} value={limits.max} onChange={(e) => setLimits({ ...limits, max: Number(e.target.value) })} /> s</label>
          <label className="check"><input type="checkbox" checked={limits.allow_overlap} onChange={(e) => setLimits({ ...limits, allow_overlap: e.target.checked })} /> Allow overlaps</label>
          <button className="primary" disabled={!!busy || !hasTranscript} onClick={() => run("Claude is picking clips", async () => {
            const r = await api.suggestShorts(video.id, limits); setFile(await api.shorts(video.id));
            if (r.rejected.length) setMsg({ text: `Skipped ${r.rejected.length} suggestion(s) that broke the limits.` });
          })}>Suggest clips</button>
        </div>
        {(busy || progress) && <p className="muted">{progress || `${busy}...`}</p>}
        {msg && <p className={msg.error ? "error" : "ok"}>{msg.text}</p>}
      </div>

      {clips.length > 0 && (
        <>
          <div className="row between">
            <div className="row">
              <button onClick={() => Promise.all(clips.map((c) => save({ ...c, selected: true })))}>Select all</button>
              <button onClick={() => Promise.all(clips.map((c) => save({ ...c, selected: false })))}>Deselect</button>
            </div>
            <div className="row">
              <select value={framing} onChange={(e) => setFraming(e.target.value as Framing)}>
                <option value="blur_fit">Blurred background (whole frame)</option>
                <option value="center">Center crop</option>
                <option value="split" disabled={!video.bubble}>Screen on top, camera below{video.bubble ? "" : " (no camera bubble in this video)"}</option>
              </select>
              <button className="primary" disabled={!!busy || !selected.length} onClick={() => run(`Rendering ${selected.length} shorts`, async () => {
                const r = await api.renderShorts(video.id, selected.map((c) => c.id), framing); setRendered(r.files);
                setMsg({ text: r.note ?? `Rendered ${r.files.length} shorts (1080x1920).` });
              })}>Render {selected.length} selected</button>
            </div>
          </div>
          <div className="clip-grid">
            {clips.map((c) => <ClipCard key={c.id} clip={c} src={srv ? `${srv}/media/${video.share_id}.mp4` : ""} busy={!!busy}
              onChange={save}
              onNudge={(edge, delta, toSentence) => api.nudgeShort(video.id, c.id, edge, delta, toSentence).then(setFile)}
              onReroll={() => run("Re-rolling", async () => setFile(await api.rerollShort(video.id, c.id)))} />)}
          </div>
        </>
      )}

      {rendered.length > 0 && (
        <div className="card panel">
          <h3>Rendered</h3>
          {rendered.map((r) => <div key={r.file} className="row"><code className="link">{r.file.split("/").pop()}</code><button onClick={() => api.reveal_path(r.file)}>Show in Finder</button></div>)}
        </div>
      )}
    </div>
  );
}

function ClipCard({ clip, src, busy, onChange, onNudge, onReroll }: {
  clip: Clip; src: string; busy: boolean; onChange: (c: Clip) => void;
  onNudge: (edge: "start" | "end", delta: number, toSentence: boolean) => void; onReroll: () => void;
}) {
  const v = useRef<HTMLVideoElement>(null);
  const [title, setTitle] = useState(clip.title);
  useEffect(() => setTitle(clip.title), [clip.title]);
  // Play only this clip's segment.
  const play = () => { const el = v.current; if (!el) return; el.currentTime = clip.start; el.play(); };
  const onTime = () => { const el = v.current; if (el && el.currentTime >= clip.end) el.pause(); };
  return (
    <div className={`card clip ${clip.selected ? "on" : ""}`}>
      <div className="clip-video">
        {src && <video ref={v} src={`${src}#t=${clip.start.toFixed(2)},${clip.end.toFixed(2)}`} preload="metadata" onTimeUpdate={onTime} onClick={play} crossOrigin="anonymous" />}
        <span className="score" title={clip.score_reason}>{clip.score}/10</span>
      </div>
      <div className="clip-body">
        <label className="check"><input type="checkbox" checked={clip.selected} onChange={(e) => onChange({ ...clip, selected: e.target.checked })} />
          <span className="muted small">{mmss(clip.start)} to {mmss(clip.end)} · {Math.round(clip.end - clip.start)}s</span></label>
        <input className="clip-title" value={title} onChange={(e) => setTitle(e.target.value)} onBlur={() => title !== clip.title && onChange({ ...clip, title })} />
        <p className="small">{clip.reason}</p>
        <p className="small muted">Virality: {clip.score_reason}</p>
        <div className="row wrap nudges">
          <span className="small muted">Start</span>
          <button onClick={() => onNudge("start", -0.5, false)}>-0.5s</button><button onClick={() => onNudge("start", 0.5, false)}>+0.5s</button>
          <button onClick={() => onNudge("start", 0, true)}>Sentence</button>
          <span className="small muted">End</span>
          <button onClick={() => onNudge("end", -0.5, false)}>-0.5s</button><button onClick={() => onNudge("end", 0.5, false)}>+0.5s</button>
          <button onClick={() => onNudge("end", 0, true)}>Sentence</button>
        </div>
        <div className="row"><button onClick={play}>▶ Preview</button><button disabled={busy} onClick={onReroll}>Re-roll</button></div>
        <details><summary className="small">Post caption</summary><p className="small">{clip.caption}</p></details>
      </div>
    </div>
  );
}
