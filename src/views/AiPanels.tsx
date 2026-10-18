import { useEffect, useState } from "react";
import { api, mmss, type EditPlan, type ShortPlan, type Video } from "../lib/api";

type Common = { video: Video; busy: string; run: (label: string, fn: () => Promise<unknown>, done?: string) => Promise<void> };

/** Type what you want changed; Claude proposes an edit plan you review before anything renders. */
export function PromptEdit({ video, busy, run, canUndo }: Common & { canUndo: boolean }) {
  const [prompt, setPrompt] = useState("");
  const [proposal, setProposal] = useState<{ plan: EditPlan; lines: string[] } | null>(null);
  return (
    <div className="card panel prompt-edit">
      <h3>Edit with a prompt</h3>
      <form onSubmit={(e) => { e.preventDefault(); run("Claude is planning the edit", async () => setProposal(await api.planEdit(video.id, prompt))); }}>
        <textarea rows={2} value={prompt} onChange={(e) => setPrompt(e.target.value)} maxLength={600}
          placeholder="Cut the part about pricing, speed up the setup 2x, make my voice louder..." required />
        <button className="primary" disabled={!!busy}>Plan the edit</button>
      </form>
      {proposal && (
        <div className="diff">
          <ul>{proposal.lines.map((l, i) => <li key={i}>{l}</li>)}</ul>
          {proposal.plan.explanation && <p className="muted small">{proposal.plan.explanation}</p>}
          <div className="row">
            <button className="primary" disabled={!!busy} onClick={() => run("Applying the edit", async () => { await api.applyPlan(video.id, proposal.plan); setProposal(null); setPrompt(""); }, "Edit applied. Undo is available.")}>Apply</button>
            <button onClick={() => setProposal(null)}>Discard</button>
          </div>
        </div>
      )}
      {canUndo && !proposal && <button disabled={!!busy} onClick={() => run("Undoing", () => api.undoEdit(video.id), "Last edit undone.")}>Undo last edit</button>}
    </div>
  );
}

const TARGETS = [30, 60, 120];

/** Claude keeps the sentences that carry the point, up to a target length, as a new video. */
export function MakeShorter({ video, busy, run, seek, onCreated }: Common & { seek: (t: number) => void; onCreated: (v: Video) => void }) {
  const [target, setTarget] = useState<number>(TARGETS.find((t) => t < video.duration) ?? Math.max(5, Math.round(video.duration / 2)));
  const [plan, setPlan] = useState<ShortPlan | null>(null);
  return (
    <div className="card panel">
      <h3>Make it shorter</h3>
      <div className="row wrap">
        {TARGETS.filter((t) => t < video.duration).map((t) => (
          <button key={t} className={target === t ? "chip on" : "chip"} onClick={() => { setTarget(t); setPlan(null); }}>{t < 60 ? `${t}s` : `${t / 60} min`}</button>
        ))}
        <input className="num" type="number" min={5} value={target} onChange={(e) => { setTarget(Number(e.target.value)); setPlan(null); }} /> <span className="muted">s</span>
      </div>
      <button disabled={!!busy || !video.has_transcript} onClick={() => run("Claude is picking the core", async () => setPlan(await api.planShorter(video.id, target)))}>
        {video.has_transcript ? "Preview the short version" : "Transcribe first"}
      </button>
      {plan && (
        <div className="diff">
          <p><b>{mmss(video.duration)} → {mmss(plan.length)}</b>, keeping {plan.ranges.length} {plan.ranges.length === 1 ? "part" : "parts"}:</p>
          <ul>{plan.ranges.map((r, i) => <li key={i}><a onClick={() => seek(r.start)}>{mmss(r.start)} to {mmss(r.end)}</a></li>)}</ul>
          {plan.explanation && <p className="muted small">{plan.explanation}</p>}
          <div className="row">
            <button className="primary" disabled={!!busy} onClick={() => run("Rendering the short version", async () => { const s = await api.renderShorter(video.id, plan.ranges); setPlan(null); onCreated(s); }, "Short version saved next to the original.")}>Render as a new video</button>
            <button onClick={() => setPlan(null)}>Discard</button>
          </div>
        </div>
      )}
    </div>
  );
}

/** Claude translates the captions line by line; timings stay the same. */
export function TranslateCaptions({ video, busy, run }: Common) {
  const [langs, setLangs] = useState<[string, string][]>([]);
  const [lang, setLang] = useState("hi");
  useEffect(() => { api.languages().then(setLangs); }, []);
  return (
    <div className="row">
      <select value={lang} onChange={(e) => setLang(e.target.value)}>
        {langs.map(([c, n]) => <option key={c} value={c}>{n}{video.caption_langs.includes(c) ? " ✓" : ""}</option>)}
      </select>
      <button disabled={!!busy || !video.has_transcript} onClick={() => run("Claude is translating the captions", () => api.translate(video.id, lang), "Caption track added.")}>Translate captions</button>
    </div>
  );
}
