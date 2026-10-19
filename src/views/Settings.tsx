import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { CircleCheck, CircleDashed, Smartphone } from "lucide-react";
import { forgetPhone, pairedPhones } from "../lib/phoneLink";
import type { PairedPhone } from "../lib/api";
import { api, brand, errText, type AiStatus, type Provider, type Tools, type Voice, type WhisperModel } from "../lib/api";
import { Button, Group, Icon, Row, Segmented, Sheet, Slider, TaskStatus, type Status } from "../components/ui";

type VoiceMode = "off" | "auto" | "gain";

export default function Settings({ onClose, onConnectPhone }: { onClose: () => void; onConnectPhone: () => void }) {
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [tools, setTools] = useState<Tools | null>(null);
  const [provider, setProvider] = useState<Provider>("cli");
  const [keySaved, setKeySaved] = useState(false);
  const [key, setKey] = useState("");
  const [voiceDefault, setVoiceDefault] = useState<Voice>({ gain_db: 0, auto: false });
  const [task, setTask] = useState<{ at: string; label: string } | null>(null);
  const [msg, setMsg] = useState<{ at: string } & NonNullable<Status> | null>(null);

  const refresh = () => {
    api.aiStatus().then(setStatus);
    api.tools().then(setTools);
    api.settings().then((s) => { setProvider(s.provider); setKeySaved(!!s.api_key); setVoiceDefault(s.voice_default); });
  };
  useEffect(refresh, []);

  const act = async (at: string, label: string, fn: () => Promise<string | void>) => {
    setTask({ at, label }); setMsg(null);
    try { const done = await fn(); refresh(); if (done) setMsg({ at, text: done }); }
    catch (e) { setMsg({ at, text: errText(e), error: true }); }
    finally { setTask(null); }
  };
  const report = (at: string) => ({ busy: task?.at === at ? task.label : undefined, status: msg?.at === at ? msg : null });
  const save = (p: Provider, k: string | null, vd?: Voice) => act("claude", "Saving", async () => { await api.saveSettings(p, k, vd); setKey(""); return "Saved."; });

  const mode: VoiceMode = voiceDefault.auto ? "auto" : voiceDefault.gain_db ? "gain" : "off";
  const setMode = (m: VoiceMode) => {
    const vd = m === "auto" ? { gain_db: 0, auto: true } : m === "gain" ? { gain_db: voiceDefault.gain_db || 3, auto: false } : { gain_db: 0, auto: false };
    setVoiceDefault(vd); api.saveSettings(provider, null, vd);
  };
  const cli = !status?.cli_installed ? "Not installed. Install Claude Code and run claude once to sign in."
    : status.cli_logged_in === false ? `Found ${status.cli_version}, not signed in. Run claude once in Terminal.` : `Found ${status.cli_version}, signed in.`;

  const installModel = async () => {
    const path = await openDialog({ multiple: false, filters: [{ name: "Speech model", extensions: ["bin"] }] });
    if (typeof path === "string") act("model", "Installing the model", async () => { await api.installModel(path, path.includes("small") ? "small_en" : "base_en"); return "Model installed."; });
  };

  return (
    <Sheet title="Settings" wide onClose={onClose} footer={<Button large kind="primary" onClick={onClose}>Done</Button>}>
      <div>
        <h3 className="section-head">Claude</h3>
        <Group>
          <Row label="Connect with">
            <Segmented label="Connect Claude with" value={provider} onChange={(p) => { setProvider(p); save(p, null); }}
              options={[{ value: "cli", label: "Claude Code" }, { value: "api", label: "API Key" }]} />
          </Row>
          {provider === "cli" ? (
            <Row label="Claude Code" detail={status ? cli : "Checking..."} />
          ) : (
            <Row label="API key" detail={keySaved ? "Saved in the Keychain." : status?.api_key_set ? "Using ANTHROPIC_API_KEY from the environment." : "Stored in the macOS Keychain, never in a file."}>
              <input className="field" type="password" style={{ width: 180 }} value={key} placeholder={keySaved ? "Replace key" : "sk-ant-"} aria-label="Anthropic API key" onChange={(e) => setKey(e.target.value)} />
              <Button disabled={!key.trim()} onClick={() => save("api", key)}>Save</Button>
              {keySaved && <Button kind="destructive" onClick={() => act("claude", "Forgetting", async () => { await api.forgetKey(); return "Key removed from the Keychain."; })}>Forget</Button>}
            </Row>
          )}
          <Row label="Model" detail={status?.model}>
            <Button disabled={!!task} onClick={() => act("claude", "Testing", () => api.testClaude())}>Test Connection</Button>
          </Row>
        </Group>
        <p className="section-foot">{brand.name} never sees your Claude login. It asks your own installed Claude Code, or uses an API key you provide.</p>
        <div className="after"><TaskStatus {...report("claude")} /></div>
      </div>

      <Phones onConnect={onConnectPhone} />

      <div>
        <h3 className="section-head">New Recordings</h3>
        <Group>
          <Row label="Voice level" detail="Applied when a recording is saved. The original audio is kept.">
            <Segmented label="Voice level" value={mode} onChange={setMode} options={[{ value: "off", label: "Off" }, { value: "auto", label: "Auto" }, { value: "gain", label: "Fixed" }]} />
          </Row>
          {mode === "gain" && (
            <Row block label={<span className="hstack"><span className="spacer">Gain</span><b className="num">{voiceDefault.gain_db > 0 ? "+" : ""}{voiceDefault.gain_db} dB</b></span>}>
              <Slider label="Default gain" min={-12} max={12} value={voiceDefault.gain_db} ticks={["-12", "-6", "0", "+6", "+12"]}
                onChange={(g) => { const vd = { gain_db: g || 1, auto: false }; setVoiceDefault(vd); api.saveSettings(provider, null, vd); }} />
            </Row>
          )}
        </Group>
      </div>

      <div>
        <h3 className="section-head">Transcription</h3>
        <Group>
          <Row label="Speech model" detail={tools?.model ? "Installed. Runs on this Mac." : `Downloads on first use, about ${tools?.model_mb ?? 150} MB.`}>
            <Segmented label="Speech model" value={tools?.model_name ?? "base_en"} onChange={(m: WhisperModel) => api.saveSettings(provider, null, undefined, { whisperModel: m }).then(refresh)}
              options={[{ value: "base_en", label: "Base", title: "English, about 150 MB, faster" }, { value: "small_en", label: "Small", title: "English, about 490 MB, more accurate" }]} />
          </Row>
          <Row label="Offline install" detail="Use a ggml-base.en.bin or ggml-small.en.bin you already have.">
            <Button disabled={!!task} onClick={installModel}>Choose File{"…"}</Button>
          </Row>
        </Group>
        <div className="after"><TaskStatus {...report("model")} /></div>
      </div>

      <div>
        <h3 className="section-head">This Mac</h3>
        <Group>
          <Check ok={tools?.ffmpeg} label="Recording and editing engine" />
          <Check ok={tools?.captions_burn} label="Burned-in captions" />
          <Check ok={tools?.cloudflared} label="Public links" />
          <Check ok={tools?.ytdlp} label="YouTube importer">
            <Button disabled={!!task} onClick={() => act("tools", "Updating", async () => `YouTube importer updated to ${await api.updateYoutube()}.`)}>Update</Button>
          </Check>
          <Row label="Library" detail={<span className="mono selectable">{tools?.library}</span>} />
          <Row label="Permissions" detail="Opens the matching pane in System Settings.">
            <Button onClick={() => api.privacy("screen")}>Screen</Button>
            <Button onClick={() => api.privacy("microphone")}>Microphone</Button>
            <Button onClick={() => api.privacy("camera")}>Camera</Button>
          </Row>
        </Group>
        <div className="after"><TaskStatus {...report("tools")} /></div>
      </div>
    </Sheet>
  );
}

function Check({ ok, label, children }: { ok?: boolean; label: string; children?: React.ReactNode }) {
  return (
    <Row label={<span className="hstack"><span className={ok ? "ok" : "secondary"}><Icon as={ok ? CircleCheck : CircleDashed} label={ok ? "Ready" : "Missing"} /></span>{label}</span>}
      detail={ok ? "Bundled" : "Not found"}>{children}</Row>
  );
}

/** Phones that connected before reconnect from their Home Screen icon. Forget stops that at once. */
function Phones({ onConnect }: { onConnect: () => void }) {
  const [list, setList] = useState<PairedPhone[] | null>(null);
  const load = () => { pairedPhones().then(setList).catch(() => setList([])); };
  useEffect(load, []);
  const when = (ms: number) => new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  return (
    <div>
      <h3 className="section-head">Phones</h3>
      <Group>
        {list?.length === 0 && <Row label="No phones yet" detail="Phones you connect are remembered here." />}
        {list?.map((p) => (
          <Row key={p.id} label={<span className="hstack"><span className="secondary"><Icon as={Smartphone} /></span>{p.name}</span>}
            detail={`${p.role === "angle" ? "Second angle" : "Face cam"}, last used ${when(p.last_seen_ms)}`}>
            <Button kind="destructive" onClick={() => forgetPhone(p.id).then(load)}>Forget</Button>
          </Row>
        ))}
        <Row label="Add a phone"><Button onClick={onConnect}>Connect Phone{"\u2026"}</Button></Row>
      </Group>
    </div>
  );
}
