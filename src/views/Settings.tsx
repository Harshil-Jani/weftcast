import { useEffect, useState } from "react";
import { api, brand, errText, type AiStatus, type Provider, type Tools } from "../lib/api";

export default function Settings() {
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [tools, setTools] = useState<Tools | null>(null);
  const [provider, setProvider] = useState<Provider>("cli");
  const [keySaved, setKeySaved] = useState(false);
  const [key, setKey] = useState("");
  const [msg, setMsg] = useState<{ text: string; error?: boolean } | null>(null);
  const [testing, setTesting] = useState(false);

  const refresh = () => {
    api.aiStatus().then(setStatus);
    api.tools().then(setTools);
    api.settings().then((s) => { setProvider(s.provider); setKeySaved(!!s.api_key); });
  };
  useEffect(refresh, []);

  const save = async (p: Provider, k: string | null) => {
    try { await api.saveSettings(p, k); setKey(""); refresh(); setMsg({ text: "Saved." }); }
    catch (e) { setMsg({ text: errText(e), error: true }); }
  };
  const test = async () => {
    setTesting(true); setMsg(null);
    try { setMsg({ text: await api.testClaude() }); } catch (e) { setMsg({ text: errText(e), error: true }); }
    finally { setTesting(false); }
  };

  return (
    <div className="settings">
      <h1 className="page-title">Settings</h1>

      <div className="card panel">
        <h3>Claude</h3>
        <p className="muted small">{brand.name} never sees your Claude login. It either asks your own installed Claude Code, or uses an API key you provide.</p>
        <label className="check"><input type="radio" checked={provider === "cli"} onChange={() => { setProvider("cli"); save("cli", null); }} />
          Use my Claude Code login (the <code>claude</code> command)</label>
        <p className="small status">{status?.cli_installed ? `Found ${status.cli_version}` : "claude is not installed. Install Claude Code and run `claude` once to log in."}</p>
        <label className="check"><input type="radio" checked={provider === "api"} onChange={() => { setProvider("api"); save("api", null); }} />
          Use an Anthropic API key</label>
        {provider === "api" && (
          <div className="row">
            <input type="password" value={key} placeholder={keySaved ? "A key is saved" : status?.api_key_set ? "Using ANTHROPIC_API_KEY" : "sk-ant-..."} onChange={(e) => setKey(e.target.value)} />
            <button onClick={() => save("api", key)}>Save key</button>
          </div>
        )}
        <div className="row">
          <button className="primary" disabled={testing} onClick={test}>{testing ? "Testing..." : "Test connection"}</button>
          <span className="muted small">Model: {status?.model}</span>
        </div>
        {msg && <p className={msg.error ? "error" : "ok"}>{msg.text}</p>}
      </div>

      <div className="card panel">
        <h3>This computer</h3>
        <ul className="checks">
          <Check ok={tools?.ffmpeg} label="ffmpeg (recording and editing)" fix="brew install ffmpeg" />
          <Check ok={tools?.whisper} label="whisper-cli (local transcripts)" fix="brew install whisper-cpp" />
          <Check ok={tools?.model} label="Speech model (downloads on first transcript, about 150 MB)" />
          <Check ok={tools?.cloudflared} label="cloudflared (public links)" fix="brew install cloudflared" />
        </ul>
        <p className="muted small">Library folder: <code>{tools?.library}</code></p>
        <div className="row wrap">
          <button onClick={() => api.privacy("screen")}>Screen Recording permission</button>
          <button onClick={() => api.privacy("microphone")}>Microphone permission</button>
          <button onClick={() => api.privacy("camera")}>Camera permission</button>
        </div>
      </div>
    </div>
  );
}

function Check({ ok, label, fix }: { ok?: boolean; label: string; fix?: string }) {
  return <li className={ok ? "yes" : "no"}>{ok ? "✓" : "○"} {label}{!ok && fix ? <code>{fix}</code> : null}</li>;
}
