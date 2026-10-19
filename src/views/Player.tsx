import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Menu, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ask } from "@tauri-apps/plugin-dialog";
import { Captions, ChevronLeft, Download, Ellipsis, Info, Link2, Maximize, PanelRight, Pause, Play, Sparkles } from "lucide-react";
import { aiReady, api, base, brand, errText, isTyping, mmss, mmssd, parseTime, sentences, toProgress, type AiStatus, type Answer, type CutOptions, type CutPlan, type Progress, type VideoDetail } from "../lib/api";
import { Button, CopyRow, Icon, IconButton, Row, Group, Segmented, Sheet, TaskStatus, Toggle, type Status } from "../components/ui";
import { useCommand, type Command } from "../lib/recording";
import VoicePanel from "./VoicePanel";
import { LayoutPicker } from "./ConnectPhone";
import { MakeShorter, PromptEdit, TranslateCaptions } from "./AiPanels";

type Props = { id: string; command: Command; onBack: () => void; onDeleted: () => void; onOpen: (id: string) => void };
type Tab = "transcript" | "chapters" | "edit" | "voice";
const DEFAULT_CUTS: CutOptions = { remove_fillers: true, remove_silences: true, min_silence: 0.8, pad: 0.2 };

/** Runs one action at a time for this video and reports busy, progress and the result under the section that started it. */
export type Run = (at: string, label: string, fn: () => Promise<unknown>, done?: string) => Promise<void>;
export type Task = { at: string; label: string } | null;
export type Report = (at: string) => { busy?: string; progress?: Progress; status?: Status };

const stored = () => { try { return localStorage.getItem("inspector") !== "0"; } catch { return true; } };

export default function Player({ id, command, onBack, onDeleted, onOpen }: Props) {
  const [d, setD] = useState<VideoDetail | null>(null);
  const [srv, setSrv] = useState("");
  const [ai, setAi] = useState<AiStatus | null>(null);
  const [langNames, setLangNames] = useState<Record<string, string>>({});
  const [capLang, setCapLang] = useState("en");
  const [task, setTask] = useState<Task>(null);
  const [progress, setProgress] = useState<Progress>(null);
  const [status, setStatus] = useState<(Status & { at: string }) | null>(null);
  const [now, setNow] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [title, setTitle] = useState("");
  const [tab, setTab] = useState<Tab>("transcript");
  const [inspector, setInspector] = useState(stored);
  const [sharing, setSharing] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [pub, setPub] = useState<string | null>(null);
  const video = useRef<HTMLVideoElement>(null);
  const rows = useMemo(() => (d ? sentences(d.cues) : []), [d]);
  const activeRow = rows.findIndex(([a, b], i) => now >= a && (now < b + 0.3 || (rows[i + 1] && now < rows[i + 1][0])));
  const rowRefs = useRef<(HTMLButtonElement | null)[]>([]);
  // Keep the spoken sentence in view while playing (not while the user browses a paused transcript).
  useEffect(() => {
    if (playing && activeRow >= 0) rowRefs.current[activeRow]?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [activeRow, playing]);

  const load = useCallback(() => api.get(id).then((x) => { setD(x); setTitle(x.video.title); }), [id]);
  useEffect(() => {
    load();
    base().then(setSrv);
    api.languages().then((l) => setLangNames(Object.fromEntries(l)));
    api.aiStatus().then(setAi);
    api.shareStatus().then(setPub);
    const off = listen<{ id: string; stage: string; done: number; total: number }>("progress", (e) => {
      if (e.payload.id === id) setProgress(toProgress(e.payload));
    });
    return () => { off.then((f) => f()); };
  }, [id, load]);

  const toggleInspector = useCallback(() => setInspector((s) => { try { localStorage.setItem("inspector", s ? "0" : "1"); } catch { /* not persisted */ } return !s; }), []);
  useCommand(command, (name) => {
    if (name === "toggle-inspector") toggleInspector();
    if (name === "export") setExporting(true);
    if (name === "share") setSharing(true);
  });

  // Space or K plays and pauses, J and L jump 5 seconds, like most editors.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const el = video.current;
      if (!el || isTyping(e.target) || e.metaKey || e.ctrlKey || document.querySelector(".scrim")) return;
      const k = e.key.toLowerCase();
      if (k === " " || k === "k") { e.preventDefault(); if (el.paused) el.play(); else el.pause(); }
      else if (k === "j" || k === "arrowleft") { e.preventDefault(); el.currentTime = Math.max(0, el.currentTime - 5); }
      else if (k === "l" || k === "arrowright") { e.preventDefault(); el.currentTime = Math.min(el.duration || 0, el.currentTime + 5); }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);

  if (!d) {
    return (
      <>
        <div className="toolbar" data-tauri-drag-region><IconButton icon={ChevronLeft} label="Back to Library" onClick={onBack} /></div>
        <div className="empty"><span className="spinner" aria-label="Loading" /></div>
      </>
    );
  }
  const v = d.video;
  // With a phone layout, play the composited file so the preview shows it.
  const src = d.composed ? `${srv}/media/${v.share_id}.mp4?d=${v.duration}&t=${v.tracks.length}${JSON.stringify(v.phone).length}` : `${srv}/raw/${v.share_id}.mp4?d=${v.duration}`;

  const run: Run = async (at, label, fn, done) => {
    setTask({ at, label }); setStatus(null); setProgress(null);
    try { await fn(); await load(); if (done) setStatus({ at, text: done }); }
    catch (e) { setStatus({ at, text: errText(e), error: true }); }
    finally { setTask(null); setProgress(null); }
  };
  const report: Report = (at) => ({
    busy: task?.at === at ? task.label : undefined,
    progress: task?.at === at ? progress : null,
    status: status?.at === at ? status : null,
  });
  const busy = !!task;
  const seek = (s: number) => { if (video.current) { video.current.currentTime = s; video.current.play(); } };
  const ready = aiReady(ai);

  const more = async () => {
    const menu = await Menu.new({
      items: [
        { id: "finder", text: "Show in Finder", action: () => api.reveal(id) },
        { id: "restore", text: "Restore Original", enabled: d.has_original && !busy, action: () => run("toolbar", "Restoring", () => api.restore(id), "Original restored.") },
        { id: "undo", text: "Undo Last Edit", enabled: d.can_undo && !busy, action: () => run("toolbar", "Undoing", () => api.undoEdit(id), "Last edit undone.") },
        await PredefinedMenuItem.new({ item: "Separator" }),
        { id: "delete", text: "Delete…", action: async () => {
          if (await ask(`"${v.title}" and its transcript will be removed from this Mac. This can't be undone.`, { title: "Delete this recording?", kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" })) {
            await api.remove(id); onDeleted();
          }
        } },
      ],
    });
    await menu.popup();
  };

  const captionsMenu = async () => {
    const pick = (lang: string) => {
      setCapLang(lang);
      const tracks = video.current?.textTracks;
      if (tracks) for (const t of Array.from(tracks)) t.mode = lang !== "off" && t.language === lang ? "showing" : "hidden";
    };
    const langs = [["en", "English"], ...v.caption_langs.map((l) => [l, langNames[l] ?? l]), ["off", "Off"]];
    const menu = await Menu.new({ items: langs.map(([code, name]) => ({ id: `cc-${code}`, text: name, checked: capLang === code, action: () => pick(code) })) });
    await menu.popup();
  };

  return (
    <>
      <div className="toolbar" data-tauri-drag-region>
        <IconButton icon={ChevronLeft} label="Back to Library" onClick={onBack} />
        <input className="field title-field" aria-label="Title" value={title} onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") { setTitle(v.title); e.currentTarget.blur(); } }}
          onBlur={() => title.trim() && title !== v.title && run("toolbar", "Renaming", () => api.rename(id, title.trim()))} />
        <span className="spacer" data-tauri-drag-region />
        <TaskStatus {...report("toolbar")} />
        <Button icon={Link2} onClick={() => setSharing(true)} title="Share (Shift Command S)">Share</Button>
        <IconButton icon={Download} label="Export (Command E)" onClick={() => setExporting(true)} />
        <IconButton icon={Ellipsis} label="More" onClick={more} />
        <span className="sep" />
        <IconButton icon={PanelRight} label="Toggle Inspector (Option Command I)" pressed={inspector} onClick={toggleInspector} />
      </div>

      <div className={`player ${inspector ? "" : "no-inspector"}`}>
        <div className="stage">
          <div className="video-wrap">
            <video ref={video} key={src} src={srv ? src : undefined} crossOrigin="anonymous" playsInline
              poster={srv ? `${srv}/thumb/${v.share_id}.jpg?d=${v.duration}` : undefined}
              onClick={(e) => { const el = e.currentTarget; if (el.paused) el.play(); else el.pause(); }}
              onPlay={() => setPlaying(true)} onPause={() => setPlaying(false)}
              onTimeUpdate={(e) => setNow(e.currentTarget.currentTime)}>
              {d.cues.length > 0 && <track kind="captions" label="English" srcLang="en" default={capLang === "en"} src={`${srv}/captions/${v.share_id}.vtt?d=${v.duration}`} />}
              {v.caption_langs.map((l) => <track key={l} kind="captions" label={langNames[l] ?? l} srcLang={l} default={capLang === l} src={`${srv}/captions/${v.share_id}.vtt?lang=${l}&d=${v.duration}`} />)}
            </video>
          </div>
          <div className="controls">
            <IconButton icon={playing ? Pause : Play} label={playing ? "Pause (Space)" : "Play (Space)"} onClick={() => { const el = video.current; if (el) { if (el.paused) el.play(); else el.pause(); } }} />
            <span className="time">{mmss(now)}</span>
            <div className="scrub">
              <input type="range" min={0} max={v.duration || 1} step={0.1} value={Math.min(now, v.duration)} aria-label="Playhead"
                style={{ ["--pct" as string]: `${(now / (v.duration || 1)) * 100}%` }}
                onChange={(e) => { const t = Number(e.target.value); setNow(t); if (video.current) video.current.currentTime = t; }} />
              {v.chapters.map((c) => c.start > 0 && <i key={c.start} className="tick" style={{ left: `${(c.start / v.duration) * 100}%` }} title={c.title} />)}
            </div>
            <span className="time">{mmss(v.duration)}</span>
            <IconButton icon={Captions} label="Captions" disabled={!d.cues.length} pressed={d.cues.length > 0 && capLang !== "off"} onClick={captionsMenu} />
            <IconButton icon={Maximize} label="Full Screen" onClick={() => video.current?.requestFullscreen()} />
          </div>
          <div className="info">
            <p className="callout secondary num">{new Date(v.created_at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })} · {mmss(v.duration)}{v.edited ? " · Edited" : ""}</p>
            {v.summary ? <p className="summary selectable">{v.summary}</p> : v.has_transcript && <p className="secondary">No summary yet. Chapters in the inspector can write one.</p>}
          </div>
          {ai && !ready && (
            <div className="notice">
              <Icon as={Info} />
              <p><b>Claude isn't connected.</b> Recording, trimming, transcripts and sharing work without it. Titles, prompt edits, shorter versions, translations and questions need your Claude Code login or an API key in Settings.</p>
            </div>
          )}
        </div>

        <aside className="inspector" aria-label="Inspector">
          <header>
            <Segmented label="Inspector" full value={tab} onChange={setTab} options={[
              { value: "transcript", label: "Transcript" }, { value: "chapters", label: "Chapters" },
              { value: "edit", label: "Edit" }, { value: "voice", label: "Voice" },
            ]} />
          </header>
          <div className="body">
            {tab === "transcript" && (!v.has_transcript ? (
              <section>
                <h3>Transcript</h3>
                <p className="hint">Transcribed on this Mac. Nothing is uploaded.</p>
                <Button kind="primary" disabled={busy} onClick={() => run("transcript", "Transcribing", () => api.transcribe(id), "Transcript ready.")}>Transcribe</Button>
                <TaskStatus {...report("transcript")} />
              </section>
            ) : (
              <>
                <AskSection id={id} run={run} report={report} busy={busy || !ready} seek={seek} />
                <section>
                  <h3>Captions</h3>
                  <TranslateCaptions video={v} busy={busy || !ready} run={run} />
                  <Button kind="plain" disabled={busy} onClick={() => run("captions", "Transcribing", () => api.transcribe(id), "Transcript refreshed.")}>Transcribe Again</Button>
                  <TaskStatus {...report("captions")} />
                </section>
                <section>
                  <h3>Transcript</h3>
                  <div className="lines selectable">
                    {rows.map(([a, , text], i) => (
                      <button key={i} ref={(el) => { rowRefs.current[i] = el; }} type="button" className="line" aria-current={i === activeRow} onClick={() => seek(a)}><time>{mmss(a)}</time><span>{text}</span></button>
                    ))}
                  </div>
                </section>
              </>
            ))}

            {tab === "chapters" && (
              <section>
                <h3>Chapters</h3>
                {v.chapters.length > 0
                  ? <div className="lines">{v.chapters.map((c) => <button key={c.start} type="button" className="line" onClick={() => seek(c.start)}><time>{mmss(c.start)}</time><span>{c.title}</span></button>)}</div>
                  : <p className="hint">{v.has_transcript ? "No chapters yet." : "Transcribe first, then Claude can write chapters."}</p>}
                <Button icon={Sparkles} disabled={busy || !v.has_transcript || !ready}
                  onClick={() => run("chapters", "Claude is writing", () => api.insights(id), "Title, summary and chapters updated.")}>Write Title, Summary and Chapters</Button>
                <TaskStatus {...report("chapters")} />
              </section>
            )}

            {tab === "edit" && (
              <>
                {v.tracks.length > 0 && <PhoneSection video={v} run={run} report={report} busy={busy} />}
                <PromptEdit video={v} busy={busy || !ready} run={run} report={report} />
                <MakeShorter video={v} busy={busy || !ready} run={run} report={report} seek={seek} onCreated={(s) => onOpen(s.id)} />
                <CutsSection id={id} duration={v.duration} run={run} report={report} busy={busy} seek={seek} />
                <TrimSection id={id} duration={v.duration} now={now} run={run} report={report} busy={busy} />
              </>
            )}

            {tab === "voice" && <VoicePanel video={v} player={video} onSaved={() => { load(); setStatus({ at: "voice", text: "Voice applied to shares and exports." }); }} saved={report("voice").status} />}
          </div>
        </aside>
      </div>

      {exporting && <ExportSheet detail={d} onClose={() => setExporting(false)} />}
      {sharing && (() => {
        const copyLink = () => { if (pub) { writeText(`${pub}/v/${v.share_id}`); setStatus({ at: "share", text: "Link copied." }); } };
        return (
          <Sheet title="Share" subtitle={`The link plays "${v.title}" from this Mac. It works while ${brand.name} is running.`}
            onClose={() => setSharing(false)} onSubmit={pub ? copyLink : () => setSharing(false)}
            footer={pub ? <><Button large onClick={() => setSharing(false)}>Done</Button><Button large kind="primary" icon={Link2} onClick={copyLink}>Copy Link</Button></>
              : <Button large kind="primary" onClick={() => setSharing(false)}>Done</Button>}>
            <SharePanel detail={d} pub={pub} setPub={setPub} run={run} report={report} busy={busy} />
          </Sheet>
        );
      })()}
    </>
  );
}

const ANGLE_LABEL: Record<string, string> = { phone_screen: "Phone screen", phone_camera: "Phone camera", angle: "Second angle" };

/** The phone screen and second-angle layout. Saving re-renders the composited video. */
function PhoneSection({ video: v, run, report, busy }: { video: VideoDetail["video"]; run: Run; report: Report; busy: boolean }) {
  const [phone, setPhone] = useState(v.phone);
  const [angle, setAngle] = useState(v.angle);
  useEffect(() => { setPhone(v.phone); setAngle(v.angle); }, [v.phone, v.angle]);
  const has = (k: string) => v.tracks.some((t) => t.kind === k);
  const changed = JSON.stringify(phone) !== JSON.stringify(v.phone) || JSON.stringify(angle) !== JSON.stringify(v.angle);
  return (
    <section>
      <h3>Phone</h3>
      <p className="hint">{v.tracks.map((t) => `${ANGLE_LABEL[t.kind]}${t.device ? ` (${t.device})` : ""}`).join(", ")}. Recorded separately, so you can change this any time.</p>
      {has("phone_screen") && (
        <>
          <Group>
            <Row label="Show phone screen"><Toggle label="Show phone screen" checked={phone.enabled} onChange={(on) => setPhone({ ...phone, enabled: on })} /></Row>
          </Group>
          {phone.enabled && <LayoutPicker panel={phone} onChange={setPhone} />}
          {phone.ranges.length > 0 && (
            <div className="hstack">
              <span className="callout secondary">Shown only in {phone.ranges.length} {phone.ranges.length === 1 ? "range" : "ranges"} from prompt edits.</span>
              <Button kind="plain" onClick={() => setPhone({ ...phone, ranges: [] })}>Show Throughout</Button>
            </div>
          )}
        </>
      )}
      {has("angle") && (
        <Group>
          <Row label="Second angle">
            <Segmented label="Second angle" value={angle.mode} onChange={(m) => setAngle({ ...angle, mode: m })} options={[
              { value: "off", label: "Off" }, { value: "pip", label: "Inset" }, { value: "split", label: "Split" },
            ]} />
          </Row>
        </Group>
      )}
      {angle.broll.length > 0 && (
        <div className="lines">
          {angle.broll.map((b, i) => (
            <div key={i} className="line">
              <time>{mmss(b.start)}</time>
              <span className="hstack"><span className="spacer">{ANGLE_LABEL[b.track]} until {mmss(b.end)}</span>
                <Button kind="plain" onClick={() => setAngle({ ...angle, broll: angle.broll.filter((_, j) => j !== i) })}>Remove</Button></span>
            </div>
          ))}
        </div>
      )}
      <div className="hstack">
        <Button kind="primary" disabled={busy || !changed} onClick={() => run("phone", "Rendering the layout", () => api.setLayout(v.id, phone, angle), "Layout applied to shares and exports.")}>Apply Layout</Button>
        {changed && <Button kind="plain" onClick={() => { setPhone(v.phone); setAngle(v.angle); }}>Reset</Button>}
      </div>
      <TaskStatus {...report("phone")} />
    </section>
  );
}

function AskSection({ id, run, report, busy, seek }: { id: string; run: Run; report: Report; busy: boolean; seek: (t: number) => void }) {
  const [q, setQ] = useState("");
  const [answer, setAnswer] = useState<Answer | null>(null);
  return (
    <section>
      <h3>Ask This Video</h3>
      <form className="hstack" onSubmit={(e) => { e.preventDefault(); run("ask", "Claude is reading", async () => setAnswer(await api.ask(id, q))); }}>
        <input className="field grow" value={q} onChange={(e) => setQ(e.target.value)} placeholder="What was decided about..." maxLength={500} required aria-label="Question" />
        <Button kind="primary" type="submit" disabled={busy || !q.trim()}>Ask</Button>
      </form>
      <TaskStatus {...report("ask")} />
      {answer && (
        <p className="answer selectable">{answer.answer}{answer.timestamps.map((t) => <button key={t} type="button" className="link" onClick={() => seek(t)}>{mmss(t)}</button>)}</p>
      )}
    </section>
  );
}

function CutsSection({ id, duration, run, report, busy, seek }: { id: string; duration: number; run: Run; report: Report; busy: boolean; seek: (t: number) => void }) {
  const [opts, setOpts] = useState(DEFAULT_CUTS);
  const [plan, setPlan] = useState<CutPlan | null>(null);
  const set = (o: Partial<CutOptions>) => { setOpts({ ...opts, ...o }); setPlan(null); };
  return (
    <section>
      <h3>Remove Ums and Pauses</h3>
      <Group>
        <Row label="Filler words" detail="um, uh, er"><Toggle label="Remove filler words" checked={opts.remove_fillers} onChange={(on) => set({ remove_fillers: on })} /></Row>
        <Row label="Pauses over">
          <input className="field" type="number" step={0.1} min={0.3} value={opts.min_silence} aria-label="Pause length in seconds"
            disabled={!opts.remove_silences} onChange={(e) => set({ min_silence: Number(e.target.value) })} />
          <span className="secondary">s</span>
          <Toggle label="Remove pauses" checked={opts.remove_silences} onChange={(on) => set({ remove_silences: on })} />
        </Row>
      </Group>
      <div className="hstack">
        <Button disabled={busy || duration <= 0} onClick={() => run("cuts", "Finding cuts", async () => setPlan(await api.planCuts(id, opts)))}>Preview</Button>
        {plan && plan.removed.length > 0 && <Button kind="primary" disabled={busy} onClick={() => run("cuts", "Cutting", async () => { await api.applyCuts(id, opts); setPlan(null); }, "Done. Transcribe again to refresh captions.")}>Apply {plan.removed.length} Cuts</Button>}
      </div>
      <TaskStatus {...report("cuts")} />
      {plan && (
        <div className="diff">
          <p className="head num">{plan.removed.length ? `${mmss(plan.original)} to ${mmss(plan.result)}, ${plan.removed.length} cuts` : "Nothing to remove."}</p>
          {plan.removed.length > 0 && <ul>{plan.removed.slice(0, 12).map(([r, why], i) => <li key={i}><button type="button" className="link num" onClick={() => seek(Math.max(0, r.start - 1))}>{mmss(r.start)}</button> {why}</li>)}</ul>}
          {plan.removed.length > 12 && <p className="callout secondary">and {plan.removed.length - 12} more</p>}
        </div>
      )}
    </section>
  );
}

function TrimSection({ id, duration, now, run, report, busy }: { id: string; duration: number; now: number; run: Run; report: Report; busy: boolean }) {
  const [trim, setTrim] = useState<[number, number]>([0, duration]);
  useEffect(() => setTrim([0, duration]), [duration]);
  const changed = trim[0] > 0.05 || trim[1] < duration - 0.05;
  const set = (i: 0 | 1, x: number) => { const t = [...trim] as [number, number]; t[i] = Math.min(duration, Math.max(0, x)); setTrim(t); };
  const field = (i: 0 | 1, label: string) => (
    <Row label={label}>
      <TimeField value={trim[i]} label={`${label} time`} onChange={(x) => set(i, x)} />
      <Button kind="plain" onClick={() => set(i, now)}>Set to Playhead</Button>
    </Row>
  );
  return (
    <section>
      <h3>Trim</h3>
      <Group>{field(0, "Start")}{field(1, "End")}</Group>
      <div className="hstack">
        <Button kind="primary" disabled={busy || !changed || trim[0] >= trim[1]} onClick={() => run("trim", "Trimming", () => api.trim(id, trim[0], trim[1]), "Trimmed. Restore Original is in the More menu.")}>Trim to {mmss(Math.max(0, trim[1] - trim[0]))}</Button>
        {changed && <Button kind="plain" onClick={() => setTrim([0, duration])}>Reset</Button>}
      </div>
      <TaskStatus {...report("trim")} />
    </section>
  );
}

/** A time typed as m:ss.s; applied on Return or when focus leaves, reverted if it doesn't parse. */
function TimeField({ value, label, onChange }: { value: number; label: string; onChange: (s: number) => void }) {
  const [draft, setDraft] = useState(mmssd(value));
  useEffect(() => setDraft(mmssd(value)), [value]);
  const commit = () => { const t = parseTime(draft); if (t === null) setDraft(mmssd(value)); else onChange(t); };
  return (
    <input className="field time" value={draft} aria-label={label} inputMode="decimal"
      onChange={(e) => setDraft(e.target.value)} onBlur={commit} onKeyDown={(e) => { if (e.key === "Enter") commit(); }} />
  );
}

function SharePanel({ detail, pub, setPub, run, report, busy }: {
  detail: VideoDetail; pub: string | null; setPub: (u: string | null) => void; run: Run; report: Report; busy: boolean;
}) {
  const v = detail.video;
  const [srv, setSrv] = useState("");
  useEffect(() => { base().then(setSrv); }, []);
  const local = `${srv}/v/${v.share_id}`;
  const link = pub ? `${pub}/v/${v.share_id}` : null;
  return (
    <>
      <Group>
        <Row label="Public link" detail={pub ? "Anyone with the link can watch." : "Off. Only this Mac can open the link."}>
          <Toggle label="Public link" checked={!!pub} disabled={busy} onChange={(on) => run("share", on ? "Opening a public link" : "Stopping", async () => {
            if (on) { const u = await api.shareStart(); setPub(u); await writeText(`${u}/v/${v.share_id}`); } else { await api.shareStop(); setPub(null); }
          }, on ? "Public link copied." : undefined)} />
        </Row>
        <Row label="Viewers can ask questions" detail="Answers use your Claude.">
          <Toggle label="Viewers can ask questions" checked={v.public_questions} disabled={busy} onChange={(on) => run("share", "Saving", () => api.setPublicQuestions(v.id, on))} />
        </Row>
      </Group>
      {link ? <CopyRow label="Public link" value={link} /> : <CopyRow label="This Mac only" value={local} onCopy={() => writeText(local)} />}
      <TaskStatus {...report("share")} />
    </>
  );
}

type Kind = "mp4" | "captioned" | "srt" | "vtt";
function ExportSheet({ detail, onClose }: { detail: VideoDetail; onClose: () => void }) {
  const v = detail.video;
  const [kind, setKind] = useState<Kind>("mp4");
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<Status>(null);
  const kinds: { k: Kind; label: string; note: string; needs?: boolean }[] = [
    { k: "mp4", label: "Video", note: "MP4" },
    { k: "captioned", label: "Video with captions", note: "MP4, burned in", needs: true },
    { k: "srt", label: "SRT Subtitles", note: "Editors and players", needs: true },
    { k: "vtt", label: "WebVTT Subtitles", note: "Web players", needs: true },
  ];
  const go = async () => {
    setBusy(true); setStatus(null);
    try { const r = await api.exportVideo(v.id, kind); setStatus({ text: r.note ?? `Saved to Downloads: ${r.path.split("/").pop()}` }); }
    catch (e) { setStatus({ text: errText(e), error: true }); }
    finally { setBusy(false); }
  };
  return (
    <Sheet title="Export" subtitle={`Saves "${v.title}" to your Downloads folder.`} onClose={onClose} onSubmit={() => { if (!busy) go(); }}
      footer={<>
        <span className="msg"><TaskStatus busy={busy ? "Exporting" : undefined} status={status} /></span>
        <Button large onClick={onClose} title="Esc">{status && !status.error ? "Done" : "Cancel"}</Button>
        <Button large kind="primary" disabled={busy} onClick={go} title="Return">Export</Button>
      </>}>
      <div className="radio-list" role="radiogroup" aria-label="Format">
        {kinds.map(({ k, label, note, needs }) => (
          <label key={k} className={needs && !v.has_transcript ? "disabled" : ""}>
            <input type="radio" name="kind" checked={kind === k} disabled={needs && !v.has_transcript} onChange={() => setKind(k)} />
            {label}<small>{needs && !v.has_transcript ? "Needs a transcript" : note}</small>
          </label>
        ))}
      </div>
    </Sheet>
  );
}
