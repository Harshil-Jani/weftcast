import { useEffect, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Globe, Link2 } from "lucide-react";
import { api, brand, errText, mmss, type Video } from "../lib/api";
import { Button, Group, Icon, Row, TaskStatus, Toggle, type Status } from "../components/ui";

/** Every video's link in one place, and the switch that makes links work outside this Mac. */
export default function Shared({ videos, srv, onOpen }: { videos: Video[]; srv: string; onOpen: (id: string) => void }) {
  const [pub, setPub] = useState<string | null>(null);
  const [busy, setBusy] = useState("");
  const [status, setStatus] = useState<Status>(null);
  const [copied, setCopied] = useState("");
  useEffect(() => { api.shareStatus().then(setPub); }, []);

  const toggle = async (on: boolean) => {
    setBusy(on ? "Opening a public link" : "Stopping"); setStatus(null);
    try { if (on) setPub(await api.shareStart()); else { await api.shareStop(); setPub(null); } }
    catch (e) { setStatus({ text: errText(e), error: true }); }
    finally { setBusy(""); }
  };
  const copy = (v: Video) => { writeText(`${pub ?? srv}/v/${v.share_id}`); setCopied(v.id); setTimeout(() => setCopied(""), 1500); };

  return (
    <>
      <div className="toolbar" data-tauri-drag-region><h1 className="title" data-tauri-drag-region>Shared</h1></div>
      <div className="page"><div className="col">
        <div>
          <Group>
            <Row label={<span className="hstack"><Icon as={Globe} />Public links</span>}
              detail={pub ? `On. Links work anywhere while ${brand.name} is running.` : "Off. Links only open on this Mac."}>
              <Toggle label="Public links" checked={!!pub} disabled={!!busy} onChange={toggle} />
            </Row>
          </Group>
          <p className="section-foot">Videos stream from this Mac through a temporary address. Nothing is uploaded, and every link stops when you turn this off or quit.</p>
          <div className="after"><TaskStatus busy={busy || undefined} status={status} /></div>
        </div>

        <div>
          <h2 className="section-head">Links</h2>
          {videos.length === 0 ? <p className="secondary">Videos you record show up here with their links.</p> : (
            <div className="group share-rows">
              {videos.map((v) => (
                <div className="row" key={v.id}>
                  <button type="button" className="mini-thumb" aria-label={`Open ${v.title}`} onClick={() => onOpen(v.id)}
                    style={{ backgroundImage: srv ? `url(${srv}/thumb/${v.share_id}.jpg?d=${v.duration})` : undefined, border: 0 }} />
                  <div className="label"><span className="truncate">{v.title}</span><small className="num">{mmss(v.duration)} · {v.public_questions ? "Viewers can ask questions" : "Watch only"}</small></div>
                  <Button icon={Link2} onClick={() => copy(v)}>{copied === v.id ? "Copied" : pub ? "Copy Public Link" : "Copy Local Link"}</Button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div></div>
    </>
  );
}
