import { useEffect, useRef, useState } from "react";
import { api, base, errText, type Video, type Voice, type VoiceStats } from "../lib/api";

const fmt = (n: number) => (Number.isFinite(n) ? n.toFixed(1) : "silent");

/**
 * Voice loudness for one video. The gain is heard live through a Web Audio GainNode on the raw
 * audio; Auto level renders a 10 second clip to listen to. Apply renders the voiced file.
 */
export default function VoicePanel({ video, player, onSaved }: {
  video: Video; player: React.RefObject<HTMLVideoElement | null>; onSaved: (v: Video) => void;
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
    <div className="card panel voice">
      <h3>Voice</h3>
      <div className="row">
        <span>Gain</span>
        <input type="range" min={-12} max={12} step={1} value={voice.gain_db}
          onChange={(e) => setVoice({ ...voice, gain_db: Number(e.target.value) })} />
        <b className="db">{voice.gain_db > 0 ? "+" : ""}{voice.gain_db} dB</b>
      </div>
      <label className="check">
        <input type="checkbox" checked={voice.auto} onChange={(e) => setVoice({ ...voice, auto: e.target.checked })} />
        Auto level (evens out quiet and loud parts, -16 LUFS)
      </label>
      {stats && (
        <p className="small muted readout">
          Now {fmt(stats.raw.lufs)} LUFS, peak {fmt(stats.raw.true_peak)} dBTP
          {stats.applied && <> · Applied {fmt(stats.applied.lufs)} LUFS, peak {fmt(stats.applied.true_peak)} dBTP</>}
          {changed && <><br />With these settings: about {fmt(voice.auto ? -16 + voice.gain_db : stats.raw.lufs + voice.gain_db)} LUFS, peak {fmt(stats.predicted_peak)} dBTP</>}
        </p>
      )}
      {stats?.clips && <p className="warn small">This gain pushes the peak to {fmt(stats.predicted_peak)} dBTP and will clip. Lower it or turn on Auto level.</p>}
      <p className="muted small">Gain plays live in the player. {voice.auto ? "Auto level needs a quick render to hear." : ""}</p>
      <div className="row wrap">
        {voice.auto && (
          <button disabled={!!busy} onClick={() => run("Rendering preview", async () => {
            await api.voicePreview(video.id, voice, player.current?.currentTime ?? 0);
            setPreviewSrc(`${await base()}/preview/${video.share_id}.mp4?t=${Date.now()}`);
          })}>Preview 10s</button>
        )}
        <button className="primary" disabled={!!busy || !changed} onClick={() => run("Applying", async () => onSaved(await api.setVoice(video.id, voice)))}>
          {busy || "Apply"}
        </button>
        {changed && <button className="ghost" onClick={() => setVoice(video.voice)}>Reset</button>}
      </div>
      {previewSrc && <audio className="preview" src={previewSrc} controls autoPlay crossOrigin="anonymous" />}
      {err && <p className="error small">{err}</p>}
    </div>
  );
}
