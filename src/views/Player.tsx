import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { aiReady, api, base, errText, mmss, type AiStatus, type Answer, type CutOptions, type CutPlan, type VideoDetail } from "../lib/api";
import VoicePanel from "./VoicePanel";
import { MakeShorter, PromptEdit, TranslateCaptions } from "./AiPanels";

type Props = { id: string; onBack: () => void; onDeleted: () => void; onOpen: (id: string) => void };
const DEFAULT_CUTS: CutOptions = { remove_fillers: true, remove_silences: true, min_silence: 0.8, pad: 0.2 };

export default function Player({ id, onBack, onDeleted, onOpen }: Props) {
  const [capLang, setCapLang] = useState("en");
  const [langNames, setLangNames] = useState<Record<string, string>>({});
  const [ai, setAi] = useState<AiStatus | null>(null);
  const [d, setD] = useState<VideoDetail | null>(null);
  const [srv, setSrv] = useState("");
  const [busy, setBusy] = useState("");
  const [msg, setMsg] = useState<{ text: string; error?: boolean } | null>(null);
  const [progress, setProgress] = useState("");
  const [now, setNow] = useState(0);
  const [title, setTitle] = useState("");
  const [publicUrl, setPublicUrl] = useState<string | null>(null);
  const [cutOpts, setCutOpts] = useState(DEFAULT_CUTS);
  const [plan, setPlan] = useState<CutPlan | null>(null);
  const [trim, setTrim] = useState<[number, number] | null>(null);
  const [q, setQ] = useState("");
  const [answer, setAnswer] = useState<Answer | null>(null);
  const video = useRef<HTMLVideoElement>(null);

  const load = useCallback(() => api.get(id).then((x) => { setD(x); setTitle(x.video.title); }), [id]);
  useEffect(() => {
    load();
    base().then(setSrv);
    api.shareStatus().then(setPublicUrl);
    api.languages().then((l) => setLangNames(Object.fromEntries(l)));
    api.aiStatus().then(setAi);
    const off = listen<{ id: string; stage: string; done: number; total: number }>("progress", (e) => {
      if (e.payload.id !== id) return;
      const { stage, done, total } = e.payload;
      setProgress(stage === "done" ? "" : total ? `${stage} ${Math.round((done / total) * 100)}%` : `${stage}...`);
    });
    return () => { off.then((f) => f()); };
  }, [id, load]);

  if (!d) return <p className="muted">Loading...</p>;
  const v = d.video;
  const src = `${srv}/raw/${v.share_id}.mp4?d=${v.duration}`;
  const localLink = `${srv}/v/${v.share_id}`;
  const publicLink = publicUrl ? `${publicUrl}/v/${v.share_id}` : null;

  const run = async (label: string, fn: () => Promise<unknown>, done?: string) => {
    setBusy(label); setMsg(null);
    try { await fn(); await load(); if (done) setMsg({ text: done }); }
    catch (e) { setMsg({ text: errText(e), error: true }); }
    finally { setBusy(""); setProgress(""); }
  };
  const seek = (s: number) => { if (video.current) { video.current.currentTime = s; video.current.play(); } };
  const activeCue = d.cues.findIndex(([a, b]) => now >= a && now < b + 0.3);

  return (
    <div className="player">
      <div className="row between">
        <button className="ghost" onClick={onBack}>← Library</button>
        <div className="row">
          <button onClick={() => api.reveal(id)}>Show in Finder</button>
          <button className="danger" onClick={() => { if (confirm("Delete this recording for good?")) run("delete", async () => { await api.remove(id); onDeleted(); }); }}>Delete</button>
        </div>
      </div>

      <div className="player-grid">
        <section>
          <video ref={video} key={src} src={srv ? src : undefined} controls poster={srv ? `${srv}/thumb/${v.share_id}.jpg?d=${v.duration}` : undefined}
            onTimeUpdate={(e) => setNow(e.currentTarget.currentTime)} crossOrigin="anonymous">
            {d.cues.length > 0 && <track kind="captions" label="English" srcLang="en" default={capLang === "en"} src={`${srv}/captions/${v.share_id}.vtt?d=${v.duration}`} />}
            {v.caption_langs.map((l) => <track key={l} kind="captions" label={langNames[l] ?? l} srcLang={l} default={capLang === l} src={`${srv}/captions/${v.share_id}.vtt?lang=${l}&d=${v.duration}`} />)}
          </video>
          {v.caption_langs.length > 0 && (
            <div className="row caption-picker">
              <span className="muted small">Captions</span>
              <select value={capLang} onChange={(e) => {
                setCapLang(e.target.value);
                const tracks = video.current?.textTracks;
                if (tracks) for (const t of Array.from(tracks)) t.mode = e.target.value !== "off" && t.language === e.target.value ? "showing" : "hidden";
              }}>
                <option value="en">English</option>
                {v.caption_langs.map((l) => <option key={l} value={l}>{langNames[l] ?? l}</option>)}
                <option value="off">Off</option>
              </select>
            </div>
          )}
          <input className="title-input" value={title} onChange={(e) => setTitle(e.target.value)}
            onBlur={() => title !== v.title && run("rename", () => api.rename(id, title))} />
          <p className="muted small">{mmss(v.duration)} · {new Date(v.created_at).toLocaleString()}{v.edited ? " · edited" : ""}</p>
          {v.summary && <p className="summary">{v.summary}</p>}
          {msg && <p className={msg.error ? "error" : "ok"}>{msg.text}</p>}
          {(busy || progress) && <p className="muted">{progress || `${busy}...`}</p>}

          {ai && !aiReady(ai) && (
            <p className="card notice">Claude isn't connected, so the AI buttons (titles, prompt edits, shorter versions, translations, questions) won't work yet. Everything else does. Connect your Claude Code login or an API key in Settings.</p>
          )}
          <div className="actions">
            <PromptEdit video={v} busy={busy} run={run} canUndo={d.can_undo} />
            <MakeShorter video={v} busy={busy} run={run} seek={seek} onCreated={(s) => onOpen(s.id)} />

            <Panel title="Transcript and AI">
              {!v.has_transcript
                ? <button className="primary" disabled={!!busy} onClick={() => run("Transcribing", () => api.transcribe(id), "Transcript ready.")}>Transcribe (on this computer)</button>
                : <button className="primary" disabled={!!busy} onClick={() => run("Claude is writing", () => api.insights(id), "Title, summary and chapters updated.")}>Write title, summary and chapters</button>}
              {v.has_transcript && <button disabled={!!busy} onClick={() => run("Transcribing", () => api.transcribe(id), "Transcript refreshed.")}>Transcribe again</button>}
              {v.has_transcript && <TranslateCaptions video={v} busy={busy} run={run} />}
            </Panel>

            <VoicePanel video={v} player={video} onSaved={() => { load(); setMsg({ text: "Voice applied to shares and exports." }); }} />

            <Panel title="Remove ums and silences">
              <label className="check"><input type="checkbox" checked={cutOpts.remove_fillers} onChange={(e) => { setCutOpts({ ...cutOpts, remove_fillers: e.target.checked }); setPlan(null); }} /> Filler words (um, uh)</label>
              <label className="check"><input type="checkbox" checked={cutOpts.remove_silences} onChange={(e) => { setCutOpts({ ...cutOpts, remove_silences: e.target.checked }); setPlan(null); }} /> Pauses longer than
                <input className="num" type="number" step="0.1" min="0.3" value={cutOpts.min_silence} onChange={(e) => { setCutOpts({ ...cutOpts, min_silence: Number(e.target.value) }); setPlan(null); }} />s</label>
              <div className="row">
                <button disabled={!!busy} onClick={() => run("Finding cuts", async () => setPlan(await api.planCuts(id, cutOpts)))}>Preview</button>
                {plan && plan.removed.length > 0 && <button className="primary" disabled={!!busy} onClick={() => run("Cutting", async () => { await api.applyCuts(id, cutOpts); setPlan(null); }, "Done. Transcribe again to refresh captions.")}>Apply</button>}
              </div>
              {plan && (
                <div className="plan">
                  <p>{plan.removed.length ? `${plan.removed.length} cuts: ${mmss(plan.original)} → ${mmss(plan.result)}` : "Nothing to remove."}</p>
                  <ul>{plan.removed.slice(0, 12).map(([r, why], i) => <li key={i}><a onClick={() => seek(Math.max(0, r.start - 1))}>{mmss(r.start)}</a> {why}</li>)}</ul>
                </div>
              )}
            </Panel>

            <Panel title="Trim">
              {!trim ? <button onClick={() => setTrim([0, v.duration])}>Trim start and end</button> : (
                <>
                  <div className="row">
                    <span>Start</span><input className="num" type="number" step="0.1" value={trim[0].toFixed(1)} onChange={(e) => setTrim([Number(e.target.value), trim[1]])} />
                    <button onClick={() => setTrim([now, trim[1]])}>Use playhead</button>
                  </div>
                  <div className="row">
                    <span>End</span><input className="num" type="number" step="0.1" value={trim[1].toFixed(1)} onChange={(e) => setTrim([trim[0], Number(e.target.value)])} />
                    <button onClick={() => setTrim([trim[0], now])}>Use playhead</button>
                  </div>
                  <div className="row">
                    <button onClick={() => setTrim(null)}>Cancel</button>
                    <button className="primary" disabled={!!busy} onClick={() => run("Trimming", async () => { await api.trim(id, trim[0], trim[1]); setTrim(null); }, "Trimmed.")}>Apply trim</button>
                  </div>
                </>
              )}
              {d.has_original && <button disabled={!!busy} onClick={() => run("Restoring", () => api.restore(id), "Original restored.")}>Restore original</button>}
            </Panel>

            <Panel title="Share">
              <div className="row"><code className="link">{localLink}</code><button onClick={() => writeText(localLink)}>Copy</button></div>
              {publicLink ? (
                <>
                  <div className="row"><code className="link">{publicLink}</code><button onClick={() => writeText(publicLink)}>Copy</button></div>
                  <p className="muted small">Works while this app is running. Anyone with the link can watch.</p>
                  <button onClick={() => run("Stopping", async () => { await api.shareStop(); setPublicUrl(null); })}>Stop public link</button>
                </>
              ) : (
                <button className="primary" disabled={!!busy} onClick={() => run("Opening a public link", async () => { const u = await api.shareStart(); setPublicUrl(u); await writeText(`${u}/v/${v.share_id}`); }, "Public link copied.")}>Get public link</button>
              )}
              <label className="check"><input type="checkbox" checked={v.public_questions} onChange={(e) => run("Saving", () => api.setPublicQuestions(id, e.target.checked))} /> Let viewers ask the video questions (uses your Claude)</label>
            </Panel>

            <Panel title="Export to Downloads">
              <div className="row wrap">
                <button disabled={!!busy} onClick={() => run("Exporting", () => api.exportVideo(id, "mp4"), "Saved to Downloads.")}>MP4</button>
                <button disabled={!!busy || !v.has_transcript} onClick={() => run("Exporting", async () => { const r = await api.exportVideo(id, "captioned"); if (r.note) setMsg({ text: r.note }); })}>MP4 with captions</button>
                <button disabled={!v.has_transcript} onClick={() => run("Exporting", () => api.exportVideo(id, "srt"), "Saved to Downloads.")}>SRT</button>
                <button disabled={!v.has_transcript} onClick={() => run("Exporting", () => api.exportVideo(id, "vtt"), "Saved to Downloads.")}>VTT</button>
              </div>
            </Panel>
          </div>
        </section>

        <aside>
          {v.chapters.length > 0 && (
            <div className="card side-card"><h3>Chapters</h3>
              {v.chapters.map((c) => <button key={c.start} className="line" onClick={() => seek(c.start)}><time>{mmss(c.start)}</time>{c.title}</button>)}
            </div>
          )}
          {v.has_transcript && (
            <div className="card side-card"><h3>Ask this video</h3>
              <form className="row" onSubmit={(e) => { e.preventDefault(); run("Claude is reading", async () => setAnswer(await api.ask(id, q))); }}>
                <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="What was decided about..." maxLength={500} required />
                <button className="primary" disabled={!!busy}>Ask</button>
              </form>
              {answer && <p className="answer">{answer.answer} {answer.timestamps.map((t) => <a key={t} onClick={() => seek(t)}>[{mmss(t)}]</a>)}</p>}
            </div>
          )}
          {d.cues.length > 0 && (
            <div className="card side-card transcript"><h3>Transcript</h3>
              {d.cues.map(([a, , text], i) => <button key={i} className={`line ${i === activeCue ? "on" : ""}`} onClick={() => seek(a)}><time>{mmss(a)}</time>{text}</button>)}
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}

function Panel({ title, children }: { title: string; children: React.ReactNode }) {
  return <div className="card panel"><h3>{title}</h3>{children}</div>;
}
