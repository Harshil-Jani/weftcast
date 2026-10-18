import { useEffect, useState } from "react";
import { CircleCheck, KeyRound, Terminal } from "lucide-react";
import { api, brand, errText, type AiStatus } from "../lib/api";
import { Button, Group, Icon, Row, Sheet, TaskStatus, type Status } from "../components/ui";

/** First run: connect Claude (your own Claude Code login or an API key), or skip. */
export default function Onboarding({ onDone }: { onDone: () => void }) {
  const [st, setSt] = useState<AiStatus | null>(null);
  const [key, setKey] = useState("");
  const [msg, setMsg] = useState<Status>(null);
  const check = () => api.aiStatus().then(setSt);
  useEffect(() => { check(); }, []);

  const finish = async (provider: "cli" | "api", apiKey: string | null = null) => {
    try { await api.saveSettings(provider, apiKey, undefined, { onboarded: true }); onDone(); }
    catch (e) { setMsg({ text: errText(e), error: true }); }
  };
  const cliOk = !!st?.cli_installed && st.cli_logged_in === true;
  const cliDetail = !st ? "Checking..." : cliOk ? `Installed and signed in, version ${st.cli_version}.`
    : st.cli_installed ? "Installed, not signed in. Run claude in Terminal, sign in, then check again."
    : "Not installed. Get it at claude.com/claude-code, run claude once, then check again.";

  return (
    <Sheet title={`Welcome to ${brand.name}`} wide onClose={() => finish(st?.api_key_set ? "api" : "cli")}
      subtitle="Recording, the library, trimming, transcripts, captions and sharing work right away on this Mac. Titles, summaries, prompt edits and translations use Claude, with your own account."
      footer={<><span className="msg"><TaskStatus status={msg} /></span><Button large kind="plain" onClick={() => finish(st?.api_key_set ? "api" : "cli")}>Skip for Now</Button></>}>
      <div>
        <h3 className="section-head">Use your Claude Code login</h3>
        <Group>
          <Row label={<span className="hstack"><span className={cliOk ? "ok" : "secondary"}><Icon as={cliOk ? CircleCheck : Terminal} /></span>Claude Code</span>} detail={cliDetail}>
            <Button onClick={check}>Check Again</Button>
            <Button kind="primary" disabled={!cliOk} onClick={() => finish("cli")}>Use Claude Code</Button>
          </Row>
        </Group>
        <p className="section-foot">{brand.name} runs your own claude command. It never sees or stores your login.</p>
      </div>
      <div>
        <h3 className="section-head">Or use an Anthropic API key</h3>
        <Group>
          <Row label={<span className="hstack"><span className="secondary"><Icon as={KeyRound} /></span>API key</span>} detail="Stored in the macOS Keychain, never in a file.">
            <input className="field" type="password" style={{ width: 200 }} placeholder="sk-ant-" aria-label="Anthropic API key" value={key} onChange={(e) => setKey(e.target.value)} />
            <Button kind="primary" disabled={!key.trim()} onClick={() => finish("api", key)}>Save</Button>
          </Row>
        </Group>
      </div>
    </Sheet>
  );
}
