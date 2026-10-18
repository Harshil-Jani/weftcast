import { useEffect, useState } from "react";
import { api, mmss, type EditPlan, type ShortPlan, type Video } from "../lib/api";
import { Button, Segmented, TaskStatus } from "../components/ui";
import type { Report, Run } from "./Player";

type Common = { video: Video; busy: boolean; run: Run };

/** Type what you want changed; Claude proposes an edit plan you review before anything renders. */
export function PromptEdit({ video, busy, run, report }: Common & { report: Report }) {
  const [prompt, setPrompt] = useState("");
  const [proposal, setProposal] = useState<{ plan: EditPlan; lines: string[] } | null>(null);
  return (
    <section>
      <h3>Edit With a Prompt</h3>
      <form className="stack" onSubmit={(e) => { e.preventDefault(); run("prompt", "Claude is planning the edit", async () => setProposal(await api.planEdit(video.id, prompt))); }}>
        <textarea className="field" rows={3} value={prompt} onChange={(e) => setPrompt(e.target.value)} maxLength={600} aria-label="What to change"
          placeholder="Cut the part about pricing, speed up the setup 2x, make my voice louder" required />
        <div className="hstack"><Button kind="primary" type="submit" disabled={busy || !prompt.trim()}>Plan the Edit</Button></div>
      </form>
      <TaskStatus {...report("prompt")} />
      {proposal && (
        <div className="diff">
          <p className="head">Proposed changes</p>
          <ul>{proposal.lines.map((l, i) => <li key={i}>{l}</li>)}</ul>
          {proposal.plan.explanation && <p className="callout secondary">{proposal.plan.explanation}</p>}
          <div className="hstack">
            <Button kind="primary" disabled={busy} onClick={() => run("prompt", "Applying the edit", async () => { await api.applyPlan(video.id, proposal.plan); setProposal(null); setPrompt(""); }, "Edit applied. Undo is in the More menu.")}>Apply</Button>
            <Button onClick={() => setProposal(null)}>Discard</Button>
          </div>
        </div>
      )}
    </section>
  );
}

const TARGETS = [30, 60, 120];
const label = (t: number) => (t < 60 ? `${t}s` : `${t / 60} min`);

/** Claude keeps the sentences that carry the point, up to a target length, as a new video. */
export function MakeShorter({ video, busy, run, report, seek, onCreated }: Common & { report: Report; seek: (t: number) => void; onCreated: (v: Video) => void }) {
  const options = TARGETS.filter((t) => t < video.duration);
  const [target, setTarget] = useState<number>(options[0] ?? Math.max(5, Math.round(video.duration / 2)));
  const [plan, setPlan] = useState<ShortPlan | null>(null);
  return (
    <section>
      <h3>Make It Shorter</h3>
      <div className="hstack">
        {options.length > 0 && (
          <Segmented label="Target length" value={String(options.includes(target) ? target : "")} onChange={(t) => { setTarget(Number(t)); setPlan(null); }}
            options={options.map((t) => ({ value: String(t), label: label(t) }))} />
        )}
        <input className="field" type="number" min={5} value={target} aria-label="Target length in seconds" onChange={(e) => { setTarget(Number(e.target.value)); setPlan(null); }} />
        <span className="secondary">s</span>
      </div>
      <div className="hstack">
        <Button disabled={busy || !video.has_transcript} onClick={() => run("shorter", "Claude is picking the core", async () => setPlan(await api.planShorter(video.id, target)))}>Preview</Button>
        {!video.has_transcript && <span className="callout secondary">Transcribe first.</span>}
      </div>
      <TaskStatus {...report("shorter")} />
      {plan && (
        <div className="diff">
          <p className="head num">{mmss(video.duration)} to {mmss(plan.length)}, keeping {plan.ranges.length} {plan.ranges.length === 1 ? "part" : "parts"}</p>
          <ul>{plan.ranges.map((r, i) => <li key={i}><button type="button" className="link num" onClick={() => seek(r.start)}>{mmss(r.start)} to {mmss(r.end)}</button></li>)}</ul>
          {plan.explanation && <p className="callout secondary">{plan.explanation}</p>}
          <div className="hstack">
            <Button kind="primary" disabled={busy} onClick={() => run("shorter", "Rendering the short version", async () => { const s = await api.renderShorter(video.id, plan.ranges); setPlan(null); onCreated(s); }, "Saved as a new video.")}>Save as New Video</Button>
            <Button onClick={() => setPlan(null)}>Discard</Button>
          </div>
        </div>
      )}
    </section>
  );
}

/** Claude translates the captions line by line; timings stay the same. */
export function TranslateCaptions({ video, busy, run }: Common) {
  const [langs, setLangs] = useState<[string, string][]>([]);
  const [lang, setLang] = useState("hi");
  useEffect(() => { api.languages().then(setLangs); }, []);
  return (
    <div className="hstack">
      <select className="field grow" value={lang} onChange={(e) => setLang(e.target.value)} aria-label="Caption language">
        {langs.map(([c, n]) => <option key={c} value={c}>{n}{video.caption_langs.includes(c) ? " (added)" : ""}</option>)}
      </select>
      <Button disabled={busy || !video.has_transcript} onClick={() => run("captions", "Claude is translating", () => api.translate(video.id, lang), "Caption track added.")}>Translate</Button>
    </div>
  );
}
