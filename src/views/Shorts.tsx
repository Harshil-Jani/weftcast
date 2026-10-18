import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Menu, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ChevronLeft, ChevronRight, Ellipsis, FileVideo, Play, Sparkles, Youtube } from "lucide-react";
import { api, errText, mmss, toProgress, type Clip, type Framing, type Limits, type Progress, type Rendered, type ShortsFile, type Video } from "../lib/api";
import { Button, Checkbox, Group, Icon, IconButton, Row, Segmented, TaskStatus, Toggle, type Status } from "../components/ui";
import { VideoCard } from "../components/VideoCard";
import { useCommand, type Command } from "../lib/recording";

const DEFAULT_LIMITS: Limits = { count: 5, min: 15, max: 60, allow_overlap: false };
const VIDEO_EXT = ["mp4", "mov", "m4v", "mkv", "webm"];

/** Turn one long video into vertical shorts: import, transcribe, let Claude pick clips, review, render. */
export default function Shorts({ videos, srv, onImported, command }: { videos: Video[]; srv: string; onImported: () => void; command: Command }) {
  const [id, setId] = useState<string | null>(null);
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState("");
  const [status, setStatus] = useState<Status>(null);
  const [over, setOver] = useState(false);

  const run = async (label: string, fn: () => Promise<unknown>) => {
    setBusy(label); setStatus(null);
    try { await fn(); } catch (e) { setStatus({ text: errText(e), error: true }); } finally { setBusy(""); }
  };
  const importPath = (path: string) => run("Importing", async () => { const v = await api.importFile(path); onImported(); setId(v.id); });
  const choose = async () => {
    const path = await openDialog({ multiple: false, directory: false, filters: [{ name: "Video", extensions: VIDEO_EXT }] });
    if (typeof path === "string") importPath(path);
  };

  // Import and "Make Shorts" can arrive with the command that opened this screen, so read it once here too.
  const [first] = useState(command);
  useEffect(() => {
    if (first?.name === "import") choose();
    if (first?.name.startsWith("shorts:")) setId(first.name.slice(7));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useCommand(command, (name) => {
    if (name === "import") choose();
    if (name.startsWith("shorts:")) setId(name.slice(7));
  });

  // Files dropped anywhere on the window import straight away (the drop gives real paths).
  useEffect(() => {
    if (id) return;
    const off = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "over" || e.payload.type === "enter") setOver(true);
      else if (e.payload.type === "leave") setOver(false);
      else if (e.payload.type === "drop") {
        setOver(false);
        const file = e.payload.paths.find((p) => VIDEO_EXT.includes(p.split(".").pop()?.toLowerCase() ?? ""));
        if (file) importPath(file); else setStatus({ text: "That isn't a video file this app can read (mp4, mov, m4v, mkv or webm).", error: true });
      }
    });
    return () => { off.then((f) => f()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const video = videos.find((v) => v.id === id);
  if (video) return <ShortsEditor video={video} srv={srv} onBack={() => setId(null)} />;

  return (
    <>
      <div className="toolbar" data-tauri-drag-region><h1 className="title" data-tauri-drag-region>Shorts</h1></div>
      <div className="page"><div className="col">
        <p className="secondary">Turn a long video into vertical shorts. Claude suggests moments that stand on their own, with a score and the reason for each. Nothing renders until you pick.</p>

        <div className={`drop-zone ${over ? "over" : ""}`}>
          <Icon as={FileVideo} size={36} />
          <h2 className="title3">Drop a video here</h2>
          <p>MP4, MOV, M4V, MKV or WebM. It is copied into your library.</p>
          <Button onClick={choose} disabled={!!busy}>Choose File…</Button>
        </div>

        <div>
          <h2 className="section-head">From YouTube</h2>
          <form className="hstack" onSubmit={(e) => { e.preventDefault(); run("Downloading from YouTube", async () => { const v = await api.importYoutube(url.trim()); setUrl(""); onImported(); setId(v.id); }); }}>
            <input className="field lg grow" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://www.youtube.com/watch?v=" aria-label="YouTube link" />
            <Button large icon={Youtube} type="submit" disabled={!url.trim() || !!busy}>Import</Button>
          </form>
          <p className="section-foot">Only import videos you own or have the rights to. Downloading other people's videos breaks YouTube's terms.</p>
          <div className="after"><TaskStatus busy={busy || undefined} status={status} /></div>
        </div>

        {videos.length > 0 && (
          <div>
            <h2 className="section-head">From Your Library</h2>
            <div className="lib-grid">
              {videos.map((v) => <VideoCard key={v.id} video={v} srv={srv} onOpen={() => setId(v.id)}
                detail={[mmss(v.duration), v.has_transcript ? "Transcript" : "Needs a transcript", v.source_url ? "Imported" : null].filter(Boolean).join(" · ")} />)}
            </div>
          </div>
        )}
      </div></div>
    </>
  );
}

function ShortsEditor({ video, srv, onBack }: { video: Video; srv: string; onBack: () => void }) {
  const [file, setFile] = useState<ShortsFile | null>(null);
  const [limits, setLimits] = useState<Limits>(DEFAULT_LIMITS);
  const [framing, setFraming] = useState<Framing>(video.bubble ? "split" : "blur_fit");
  const [hasTranscript, setHasTranscript] = useState(video.has_transcript);
  const [busy, setBusy] = useState("");
  const [progress, setProgress] = useState<Progress>(null);
  const [status, setStatus] = useState<Status>(null);
  const [rendered, setRendered] = useState<Rendered[]>([]);

  useEffect(() => {
    api.shorts(video.id).then((f) => { setFile(f); if (f.limits) setLimits(f.limits); });
    const off = listen<{ id: string; stage: string; done: number; total: number }>("progress", (e) => {
      if (e.payload.id === video.id) setProgress(toProgress(e.payload));
    });
    return () => { off.then((f) => f()); };
  }, [video.id]);

  const run = async (label: string, fn: () => Promise<unknown>, done?: string) => {
    setBusy(label); setStatus(null); setProgress(null);
    try { await fn(); if (done) setStatus({ text: done }); } catch (e) { setStatus({ text: errText(e), error: true }); } finally { setBusy(""); setProgress(null); }
  };
  const clips = file?.clips ?? [];
  const selected = clips.filter((c) => c.selected);
  const save = (c: Clip) => api.updateShort(video.id, c).then(setFile);
  const num = (k: "count" | "min" | "max", label: string, min: number, max?: number) => (
    <input className="field" type="number" min={min} max={max} value={limits[k]} aria-label={label} onChange={(e) => setLimits({ ...limits, [k]: Number(e.target.value) })} />
  );

  return (
    <>
      <div className="toolbar" data-tauri-drag-region>
        <IconButton icon={ChevronLeft} label="Back to Shorts" onClick={onBack} />
        <h1 className="title" data-tauri-drag-region>{video.title}</h1>
        <span className="spacer" data-tauri-drag-region />
        {!hasTranscript
          ? <Button kind="primary" disabled={!!busy} onClick={() => run("Transcribing", async () => { await api.transcribe(video.id); setHasTranscript(true); }, "Transcript ready. Now Claude can suggest clips.")}>Transcribe First</Button>
          : <Button kind={clips.length ? "default" : "primary"} icon={Sparkles} disabled={!!busy} onClick={() => run("Claude is picking clips", async () => {
              const r = await api.suggestShorts(video.id, limits); setFile(await api.shorts(video.id));
              if (r.rejected.length) setStatus({ text: `Skipped ${r.rejected.length} suggestion(s) that broke the limits.` });
            })}>{clips.length ? "Suggest Again" : "Suggest Clips"}</Button>}
        {clips.length > 0 && (
          <>
            <span className="sep" />
            <Segmented label="Framing" value={framing} onChange={setFraming} options={[
              { value: "blur_fit", label: "Fit", title: "Whole frame over a blurred background" },
              { value: "center", label: "Crop", title: "Centre crop" },
              { value: "split", label: "Split", disabled: !video.bubble, title: video.bubble ? "Screen on top, camera below" : "This video has no camera bubble" },
            ]} />
            <Button kind="primary" disabled={!!busy || !selected.length} onClick={() => run(`Rendering ${selected.length} shorts`, async () => {
              const r = await api.renderShorts(video.id, selected.map((c) => c.id), framing); setRendered(r.files);
              setStatus({ text: r.note ?? `Rendered ${r.files.length} shorts at 1080 by 1920.` });
            })}>Render {selected.length || ""} {selected.length === 1 ? "Short" : "Shorts"}</Button>
          </>
        )}
      </div>

      <div className="page"><div className="stack" style={{ gap: 20 }}>
        {video.source_url && <p className="callout secondary truncate">Imported from {video.source_url}</p>}
        <div className="limits">
          <label>Clips {num("count", "Number of clips", 1, 20)}</label>
          <label>Length {num("min", "Shortest clip in seconds", 5)} to {num("max", "Longest clip in seconds", 5)} s</label>
          <label><Toggle label="Allow overlaps" checked={limits.allow_overlap} onChange={(on) => setLimits({ ...limits, allow_overlap: on })} />Allow overlaps</label>
        </div>
        <TaskStatus busy={busy || undefined} progress={progress} status={status} />

        {file && clips.length === 0 && !busy && (
          <div className="empty" style={{ height: "auto" }}>
            <h2 className="title3">No clips yet</h2>
            <p>{hasTranscript ? "Set the limits above, then Suggest Clips in the toolbar." : "Transcribe the video first. It runs on this Mac."}</p>
          </div>
        )}

        {clips.length > 0 && (
          <>
            <div className="hstack">
              <span className="headline">{clips.length} suggestions</span>
              <span className="callout secondary">{selected.length} selected</span>
              <span className="spacer" />
              <Button kind="plain" onClick={() => Promise.all(clips.map((c) => save({ ...c, selected: true })))}>Select All</Button>
              <Button kind="plain" onClick={() => Promise.all(clips.map((c) => save({ ...c, selected: false })))}>Deselect All</Button>
            </div>
            <div className="short-grid">
              {clips.map((c) => <ShortCard key={c.id} clip={c} framing={framing} bubble={video.bubble} src={srv ? `${srv}/media/${video.share_id}.mp4` : ""} busy={!!busy}
                onChange={save}
                onNudge={(edge, delta, toSentence) => api.nudgeShort(video.id, c.id, edge, delta, toSentence).then(setFile)}
                onReroll={() => run("Finding a different clip", async () => setFile(await api.rerollShort(video.id, c.id)))} />)}
            </div>
          </>
        )}

        {rendered.length > 0 && (
          <div>
            <h2 className="section-head">Rendered</h2>
            <Group>
              {rendered.map((r) => <Row key={r.file} label={<span className="truncate mono">{r.file.split("/").pop()}</span>}><Button onClick={() => api.reveal_path(r.file)}>Show in Finder</Button></Row>)}
            </Group>
          </div>
        )}
      </div></div>
    </>
  );
}

/** A 9:16 preview of one suggested clip, framed the way it will render. */
function ShortCard({ clip, framing, bubble, src, busy, onChange, onNudge, onReroll }: {
  clip: Clip; framing: Framing; bubble: Video["bubble"]; src: string; busy: boolean; onChange: (c: Clip) => void;
  onNudge: (edge: "start" | "end", delta: number, toSentence: boolean) => void; onReroll: () => void;
}) {
  const frame = useRef<HTMLDivElement>(null);
  const [title, setTitle] = useState(clip.title);
  const [playing, setPlaying] = useState(false);
  useEffect(() => setTitle(clip.title), [clip.title]);
  const all = () => Array.from(frame.current?.querySelectorAll("video") ?? []);
  const toggle = () => {
    const vids = all();
    if (playing) { vids.forEach((v) => v.pause()); setPlaying(false); return; }
    vids.forEach((v) => { v.currentTime = clip.start; v.play(); });
    setPlaying(true);
  };
  const onTime = (e: React.SyntheticEvent<HTMLVideoElement>) => {
    if (e.currentTarget.currentTime >= clip.end) { all().forEach((v) => v.pause()); setPlaying(false); }
  };
  // The paused frame is mid-clip (a representative picture, not the often-dark first frame); Play starts at the clip start.
  const mid = (clip.start + clip.end) / 2;
  const media = `${src}#t=${mid.toFixed(2)},${clip.end.toFixed(2)}`;
  const mode = framing === "split" && !bubble ? "blur_fit" : framing;

  const menu = async () => {
    const sep = () => PredefinedMenuItem.new({ item: "Separator" });
    const m = await Menu.new({
      items: [
        { id: "s-", text: "Start 0.5s Earlier", action: () => onNudge("start", -0.5, false) },
        { id: "s+", text: "Start 0.5s Later", action: () => onNudge("start", 0.5, false) },
        { id: "ss", text: "Start at Sentence", action: () => onNudge("start", 0, true) },
        await sep(),
        { id: "e-", text: "End 0.5s Earlier", action: () => onNudge("end", -0.5, false) },
        { id: "e+", text: "End 0.5s Later", action: () => onNudge("end", 0.5, false) },
        { id: "es", text: "End at Sentence", action: () => onNudge("end", 0, true) },
        await sep(),
        { id: "copy", text: "Copy Post Caption", action: () => writeText(clip.caption) },
        { id: "reroll", text: "Find a Different Clip", enabled: !busy, action: onReroll },
      ],
    });
    await m.popup();
  };

  return (
    <div className={`short ${clip.selected ? "" : "off"}`}>
      <div ref={frame} className={`frame ${mode}`}>
        {src && mode === "blur_fit" && <video className="bg" src={media} muted preload="metadata" aria-hidden />}
        {src && <video className="fg" src={media} preload="metadata" onTimeUpdate={onTime} />}
        {src && mode === "split" && bubble && (
          <div className="cam"><div className="sq">
            <video src={media} muted preload="metadata" aria-hidden style={{
              width: `${100 / bubble[2]}%`, height: `${100 / bubble[3]}%`, left: `${(-bubble[0] / bubble[2]) * 100}%`, top: `${(-bubble[1] / bubble[3]) * 100}%`,
            }} />
          </div></div>
        )}
        <span className="pick"><Checkbox label={`Include ${clip.title}`} checked={clip.selected} onChange={(on) => onChange({ ...clip, selected: on })} /></span>
        <span className={`score ${clip.score >= 8 ? "high" : ""}`} title={clip.score_reason}>{clip.score}/10</span>
        <button type="button" className="play" aria-label={playing ? "Pause preview" : "Play preview"} onClick={toggle}>
          <span><Icon as={Play} size={20} /></span>
        </button>
      </div>
      <div className="hstack">
        <textarea className="t" rows={2} value={title} aria-label="Clip title" spellCheck={false}
          onChange={(e) => setTitle(e.target.value.replace(/\n/g, " "))}
          onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); e.currentTarget.blur(); } if (e.key === "Escape") { setTitle(clip.title); e.currentTarget.blur(); } }}
          onBlur={() => title.trim() && title !== clip.title && onChange({ ...clip, title: title.trim() })} />
        <IconButton icon={Ellipsis} label="Adjust clip" small onClick={menu} />
      </div>
      <span className="range">{mmss(clip.start)} to {mmss(clip.end)} · {Math.round(clip.end - clip.start)}s</span>
      <details>
        <summary><Icon as={ChevronRight} size={12} />Why this clip</summary>
        <p>{clip.reason}</p>
        <p>Score {clip.score}/10: {clip.score_reason}</p>
      </details>
    </div>
  );
}
