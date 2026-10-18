import { useEffect, useRef, useState } from "react";
import { api, base, errText, type Video, type Voice, type VoiceStats } from "../lib/api";
import { Button, Group, Row, Slider, TaskStatus, Toggle, type Status } from "../components/ui";

const fmt = (n: number) => (Number.isFinite(n) ? n.toFixed(1) : "silent");
const signed = (n: number) => `${n > 0 ? "+" : ""}${n} dB`;

/**
 * Voice loudness for one video. The gain is heard live through a Web Audio GainNode on the raw
 * audio; Auto level renders a 10 second clip to listen to. Apply renders the voiced file.
 */
export default function VoicePanel({ video, player, onSaved, saved }: {
  video: Video; player: React.RefObject<HTMLVideoElement | null>; onSaved: (v: Video) => void; saved?: Status;
}) {
  const [voice, setVoice] = useState<Voice>(video.voice);
  const [stats, setStats] = useState<VoiceStats | null>(null);
  const [busy, setBusy] = useState("");
  const [err, setErr] = useState("");
  const [previewSrc, setPreviewSrc] = useState("");
  const gain = useRef<GainNode | null>(null);
  const changed = voice.gain_db !== video.voice.gain_db || voice.auto !== video.voice.auto;

  // Route the player through a GainNode once (needs a user gesture to start audio).
  useEffect(() => {
    const el = player.current;
    if (!el) return;
    const attach = () => {
      if (gain.current) return;
      const ctx = new AudioContext();
      const node = ctx.createGain();
      ctx.createMediaElementSource(el).connect(node).connect(ctx.destination);
      gain.current = node;
      node.gain.value = 10 ** (voice.gain_db / 20);
    };
    el.addEventListener("play", attach);
    return () => el.removeEventListener("play", attach);
  }, [player, voice.gain_db]);

  useEffect(() => {
    if (gain.current) gain.current.gain.value = 10 ** (voice.gain_db / 20);
  }, [voice.gain_db]);

  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      api.voiceStats(video.id, voice).then((s) => live && setStats(s)).catch((e) => live && setErr(errText(e)));
    }, 250);
    return () => { live = false; clearTimeout(t); };
  }, [video.id, voice, video.voice]);

  const run = async (label: string, fn: () => Promise<void>) => {
    setBusy(label); setErr("");
    try { await fn(); } catch (e) { setErr(errText(e)); } finally { setBusy(""); }
  };

  return (
    <>
      <section>
        <h3>Voice Level</h3>
        <Group>
          <Row block label={<span className="hstack"><span className="spacer">Gain</span><b className="num">{signed(voice.gain_db)}</b></span>} detail="Plays live in the player.">
            <Slider label="Gain" min={-12} max={12} value={voice.gain_db} ticks={["-12", "-6", "0", "+6", "+12"]} onChange={(g) => setVoice({ ...voice, gain_db: g })} />
          </Row>
          <Row label="Auto level" detail="Evens out quiet and loud parts to -16 LUFS.">
            <Toggle label="Auto level" checked={voice.auto} onChange={(auto) => setVoice({ ...voice, auto })} />
          </Row>
        </Group>
      </section>

      {stats && (
        <section>
          <h3>Loudness</h3>
          <div className="meter">
            <span>Now: {fmt(stats.raw.lufs)} LUFS, peak {fmt(stats.raw.true_peak)} dBTP</span>
            {stats.applied && <span>Applied: {fmt(stats.applied.lufs)} LUFS, peak {fmt(stats.applied.true_peak)} dBTP</span>}
            {changed && <span>With these settings: about {fmt(voice.auto ? -16 + voice.gain_db : stats.raw.lufs + voice.gain_db)} LUFS, peak {fmt(stats.predicted_peak)} dBTP</span>}
          </div>
          {stats.clips && <p className="callout warn" role="alert">This gain pushes the peak to {fmt(stats.predicted_peak)} dBTP and will clip. Lower it or turn on Auto level.</p>}
        </section>
      )}

      <section>
        <div className="hstack wrap">
          <Button kind="primary" disabled={!!busy || !changed} onClick={() => run("Applying", async () => onSaved(await api.setVoice(video.id, voice)))}>Apply</Button>
          {voice.auto && (
            <Button disabled={!!busy} onClick={() => run("Rendering a preview", async () => {
              await api.voicePreview(video.id, voice, player.current?.currentTime ?? 0);
              setPreviewSrc(`${await base()}/preview/${video.share_id}.mp4?t=${Date.now()}`);
            })}>Preview 10 Seconds</Button>
          )}
          {changed && <Button kind="plain" onClick={() => setVoice(video.voice)}>Reset</Button>}
        </div>
        <TaskStatus busy={busy || undefined} status={err ? { text: err, error: true } : saved} />
        {previewSrc && <audio src={previewSrc} controls autoPlay crossOrigin="anonymous" style={{ width: "100%" }} />}
      </section>
    </>
  );
}
