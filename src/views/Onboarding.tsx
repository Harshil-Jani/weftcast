import { useEffect, useState } from "react";
import { api, brand, errText, type AiStatus } from "../lib/api";

/** First run: connect Claude (your own Claude Code login or an API key), or skip. */
export default function Onboarding({ onDone }: { onDone: () => void }) {
  const [st, setSt] = useState<AiStatus | null>(null);
  const [key, setKey] = useState("");
  const [msg, setMsg] = useState<{ text: string; error?: boolean } | null>(null);
  const check = () => api.aiStatus().then(setSt);
  useEffect(() => { check(); }, []);

  const finish = async (provider: "cli" | "api", apiKey: string | null = null) => {
    try { await api.saveSettings(provider, apiKey, undefined, { onboarded: true }); onDone(); }
    catch (e) { setMsg({ text: errText(e), error: true }); }
  };
  const cliOk = !!st?.cli_installed && st.cli_logged_in === true;

  return (
    <div className="modal-bg">
      <div className="modal onboarding">
        <h2>Welcome to {brand.name}</h2>
        <p className="muted">Recording, the library, trimming, transcripts, captions and sharing all work right away, on this computer. The smart parts (titles, summaries, edits with a prompt, translations) use Claude, with your own account.</p>
        <div className="card panel">
          <h3>Option 1: your Claude Code login</h3>
          {!st && <p className="muted small">Checking...</p>}
          {st && cliOk && <p className="ok">Claude Code is installed and signed in ({st.cli_version}).</p>}
          {st && st.cli_installed && st.cli_logged_in !== true && <p className="small">Claude Code is installed but not signed in. Open Terminal, run <code>claude</code>, sign in, then check again.</p>}
          {st && !st.cli_installed && <p className="small">Claude Code isn't installed. Get it at claude.com/claude-code, run <code>claude</code> once to sign in, then check again.</p>}
          <div className="row">
            <button onClick={check}>Check again</button>
            <button className="primary" disabled={!cliOk} onClick={() => finish("cli")}>Use Claude Code</button>
          </div>
          <p className="muted small">{brand.name} runs your own <code>claude</code> command. It never sees or stores your login.</p>
        </div>
        <div className="card panel">
          <h3>Option 2: an Anthropic API key</h3>
          <div className="row">
            <input type="password" placeholder="sk-ant-..." value={key} onChange={(e) => setKey(e.target.value)} />
            <button className="primary" disabled={!key.trim()} onClick={() => finish("api", key)}>Save in Keychain</button>
          </div>
          <p className="muted small">Stored in the macOS Keychain, never in a file.</p>
        </div>
        {msg && <p className={msg.error ? "error" : "ok"}>{msg.text}</p>}
        <div className="row end"><button className="ghost" onClick={() => finish(st?.api_key_set ? "api" : "cli")}>Skip for now</button></div>
      </div>
    </div>
  );
}
