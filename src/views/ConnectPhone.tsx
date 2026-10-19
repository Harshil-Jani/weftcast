import { useCallback, useEffect, useRef, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { Airplay, Cable, ChevronRight, CircleCheck, Globe, RefreshCw, Send, Settings2, ShieldAlert, Smartphone, Wifi } from "lucide-react";
import { api, brand, DEFAULT_PANEL, errText, loadPrefs, savePrefs, type AirplayStatus, type PairRole, type PhoneDevices, type PhoneLayout, type PhoneLink, type PhonePanel, type PhoneSize, type PhoneSource } from "../lib/api";
import { disconnectPhone, setPairRole, usePhoneStatus, type LivePhone } from "../lib/phoneLink";
import { Button, Group, Icon, Row, Segmented, Sheet, StatusDot, TaskStatus, type Status } from "../components/ui";

type Tab = "wireless" | "screen";

const ROLES: { value: PairRole; label: string }[] = [
  { value: "camera", label: "Face Cam" }, { value: "angle", label: "Second Angle" }, { value: "remote", label: "Remote" },
];
const ROLE_DETAIL: Record<PairRole, string> = {
  camera: "Your face, in the camera bubble.",
  angle: "A second camera for hands, a whiteboard or a product.",
  remote: "Start, pause and stop from the phone.",
};

/**
 * Connect a phone, over Wi-Fi first. Camera & Remote: AirDrop the link to an iPhone, or scan the
 * QR code (a short link and a 6-digit code as backups); up to four phones, each with a job.
 * Phone Screen: an iPhone mirroring with AirPlay, or an Android phone over Wireless debugging,
 * with a cable as the fallback for the best quality.
 */
export default function ConnectPhone({ onClose }: { onClose: () => void }) {
  const [tab, setTab] = useState<Tab>("wireless");
  return (
    <Sheet title="Connect Phone" wide onClose={onClose} footer={<Button large kind="primary" onClick={onClose}>Done</Button>}>
      <Segmented label="What to use the phone for" full value={tab} onChange={setTab} options={[
        { value: "wireless", label: "Camera & Remote" }, { value: "screen", label: "Phone Screen" },
      ]} />
      {tab === "wireless" ? <Wireless onConnected={onClose} /> : <ScreenMode />}
    </Sheet>
  );
}

const STEPS_OFFLINE: { icon: typeof ShieldAlert; text: string }[] = [
  { icon: ShieldAlert, text: "Safari says the connection is not private. Tap Show Details." },
  { icon: Globe, text: "Tap visit this website." },
  { icon: CircleCheck, text: "Tap Visit Website. Safari remembers it for next time." },
];

function Wireless({ onConnected }: { onConnected: () => void }) {
  const [role, setRole] = useState<PairRole>("camera");
  const [link, setLink] = useState<PhoneLink | null>(null);
  const [err, setErr] = useState<Status>(null);
  const [sent, setSent] = useState<Status>(null);
  const phone = usePhoneStatus(700);
  const count = useRef<number | null>(null);

  // Open on mount (the link starts), close on unmount (it stops unless a phone is connected).
  useEffect(() => {
    let live = true;
    api.phoneOpen("camera").then((l) => live && setLink(l)).catch((e) => live && setErr({ text: errText(e), error: true }));
    return () => { live = false; api.phoneClose(); };
  }, []);
  // A phone that joins while this is open: show it, then get out of the way.
  useEffect(() => {
    const n = phone.phones.length;
    if (count.current === null) { count.current = n; return; }
    if (n > count.current) {
      count.current = n;
      const t = window.setTimeout(onConnected, 1000);
      return () => clearTimeout(t);
    }
    count.current = n;
  }, [phone.phones.length, onConnected]);

  const nextRole = (r: PairRole) => { setRole(r); setPairRole(r); };
  const airdrop = async () => {
    if (!link) return;
    setSent(null);
    try { await api.phoneAirdrop(link.url); setSent({ text: "Pick your iPhone, then tap the notification on it." }); }
    catch (e) { setSent({ text: errText(e), error: true }); }
  };
  const code = link ? `${link.code.slice(0, 3)} ${link.code.slice(3)}` : "";
  const any = phone.phones.length > 0;
  const justJoined = phone.phones.length > 0 && count.current !== null && phone.phones.length > (count.current ?? 0);

  return (
    <>
      {any && <ConnectedPhones phones={phone.phones} />}

      <div className="pair">
        <div className="qr" role="img" aria-label={link ? "QR code to scan with your phone" : "Preparing the QR code"}>
          {link ? <span dangerouslySetInnerHTML={{ __html: link.svg }} /> : <span className="spinner" />}
        </div>
        <div className="stack pair-text">
          {justJoined ? (
            <p className="pair-state ok" role="status"><Icon as={CircleCheck} size={22} />Connected: {phone.phones[phone.phones.length - 1].device}</p>
          ) : link ? (
            <>
              <p className="title2">{any ? "Add another phone" : "Connect your phone"}</p>
              <Button large kind="primary" icon={Send} onClick={airdrop}>Send to my iPhone</Button>
              <p className="secondary">Sends the link with AirDrop. Or point any phone's camera at the code.</p>
              <TaskStatus status={sent} />
              <p className="pair-state secondary" role="status"><span className="spinner" />Waiting for your phone{"…"}</p>
            </>
          ) : (
            <>
              <p className="title2">Preparing a link{"…"}</p>
              <p className="secondary">This takes a few seconds.</p>
            </>
          )}
          {link && (
            <div className="backup">
              <p className="callout secondary">No camera? Open this on your phone and enter the code:</p>
              <p className="backup-row"><span className="mono selectable">{link.short}</span><span className="code num selectable" aria-label={`Code ${link.code.split("").join(" ")}`}>{code}</span></p>
            </div>
          )}
          <TaskStatus status={err} />
        </div>
      </div>

      {link?.mode === "local" && (
        <div>
          <p className="section-head" role="heading" aria-level={3}>This Mac is offline, so your phone asks once</p>
          <ol className="offline-steps">
            {STEPS_OFFLINE.map((s, i) => <li key={i}><span className="step-icon"><Icon as={s.icon} size={18} /></span><span>{s.text}</span></li>)}
          </ol>
        </div>
      )}

      <Group>
        <Row label={any ? "Next phone" : "Use it as"} detail={ROLE_DETAIL[role]}>
          <Segmented label="Use the next phone as" value={role} onChange={nextRole} options={ROLES} />
        </Row>
      </Group>
      <p className="section-foot">Every phone has a Remote tab too. Phones are remembered: next time, open {brand.name} from the phone's Home Screen. Manage them in Settings.</p>
    </>
  );
}

/** The phones in this session, each with its job; changing one to a taken job swaps them. */
function ConnectedPhones({ phones }: { phones: LivePhone[] }) {
  return (
    <div>
      <p className="section-head" role="heading" aria-level={3}>Connected</p>
      <Group>
        {phones.map((p) => (
          <Row key={p.id} label={<span className="hstack"><Icon as={Smartphone} />{p.device || "Phone"}<StatusDot on /></span>} detail={ROLE_DETAIL[p.role]}>
            <span className="hstack">
              <Segmented label={`Use ${p.device || "this phone"} as`} value={p.role} onChange={(r: PairRole) => setPairRole(r, p.id)} options={ROLES} />
              <Button kind="plain" onClick={() => disconnectPhone(p.id)}>Disconnect</Button>
            </span>
          </Row>
        ))}
      </Group>
    </div>
  );
}

function ScreenMode() {
  const [devs, setDevs] = useState<PhoneDevices | null>(null);
  const [air, setAir] = useState<AirplayStatus | null>(null);
  const [status, setStatus] = useState<Status>(null);
  const [busy, setBusy] = useState("");
  const [cable, setCable] = useState(false);
  const [source, setSource] = useState<PhoneSource | null>(() => loadPrefs()?.phone ?? null);
  const [panel, setPanel] = useState<PhonePanel>(() => loadPrefs()?.phone?.layout ?? DEFAULT_PANEL);
  const refresh = useCallback(() => { api.phoneDevices().then(setDevs).catch((e) => setStatus({ text: errText(e), error: true })); }, []);
  useEffect(refresh, [refresh]);
  // AirPlay: notice the iPhone starting to mirror, without a refresh button.
  useEffect(() => {
    let live = true;
    const tick = () => api.airplayStatus().then((s) => live && setAir(s)).catch(() => {});
    tick();
    const t = window.setInterval(tick, 1500);
    return () => { live = false; clearInterval(t); };
  }, []);
  const save = (src: PhoneSource | null, p: PhonePanel) => {
    const prefs = loadPrefs();
    if (prefs) savePrefs({ ...prefs, phone: src ? { ...src, layout: p } : null });
    else if (src) savePrefs({ screen: "", mic: null, camera: true, phone: { ...src, layout: p } });
  };
  const pick = (src: PhoneSource | null) => { setSource(src); save(src, panel); };
  const layout = (p: PhonePanel) => { setPanel(p); save(source, p); };
  const getTools = async () => {
    const yes = await ask(
      `${brand.name} needs Google's adb to see Android phones. It downloads it from Google (about 16 MB) and checks the file. By continuing you agree to the Android Software Development Kit License Agreement: ${devs?.adb_terms}`,
      { title: "Get Android Tools?", kind: "info", okLabel: "Agree and Download", cancelLabel: "Cancel" },
    );
    if (!yes) return;
    setBusy("Downloading Android tools"); setStatus(null);
    try { setStatus({ text: `Installed ${await api.installAndroidTools()}.` }); refresh(); }
    catch (e) { setStatus({ text: errText(e), error: true }); }
    finally { setBusy(""); }
  };
  const isOn = (s: PhoneSource) =>
    source?.kind === s.kind
    && (s.kind === "airplay"
      || (s.kind === "ios" && source.kind === "ios" && source.index === s.index)
      || (s.kind === "android" && source.kind === "android" && source.serial === s.serial));
  const airplay: PhoneSource = { kind: "airplay", name: "iPhone (AirPlay)" };
  const wifiAndroid = devs?.android.filter((d) => d.serial.includes(":")) ?? [];
  const usbAndroid = devs?.android.filter((d) => !d.serial.includes(":")) ?? [];

  return (
    <>
      <p className="callout secondary">Record the phone's screen next to yours, for app demos. It is saved as its own file, so the layout can change after recording.</p>
      <Group>
        <Row label={<span className="hstack"><Icon as={Smartphone} />No phone screen</span>}>
          <input type="radio" name="phone" aria-label="No phone screen" checked={!source} onChange={() => pick(null)} />
        </Row>
      </Group>

      <div>
        <p className="section-head" role="heading" aria-level={3}>iPhone over Wi-Fi</p>
        <Group>
          {air?.mirroring ? (
            <Row label={<span className="hstack"><Icon as={Airplay} />iPhone is mirroring<StatusDot on /></span>} detail="Its screen is recorded next to yours.">
              <input type="radio" name="phone" aria-label="Use iPhone screen (AirPlay)" checked={isOn(airplay)} onChange={() => pick(airplay)} />
            </Row>
          ) : air?.receiver === false ? (
            <Row label={<span className="hstack"><Icon as={Airplay} />Turn on AirPlay Receiver</span>} detail="System Settings, General, AirDrop & Handoff, then AirPlay Receiver.">
              <Button icon={Settings2} onClick={() => api.openAirplaySettings()}>Open Settings</Button>
            </Row>
          ) : (
            <Row block label={<span className="hstack"><Icon as={Airplay} />Mirror your iPhone to this Mac</span>}>
              <ol className="offline-steps">
                <li><span className="step-icon">1</span><span>On the iPhone, open Control Center.</span></li>
                <li><span className="step-icon">2</span><span>Tap Screen Mirroring.</span></li>
                <li><span className="step-icon">3</span><span>Pick this Mac. It shows up here on its own.</span></li>
              </ol>
            </Row>
          )}
        </Group>
      </div>

      <div>
        <p className="section-head" role="heading" aria-level={3}>Android over Wi-Fi</p>
        {devs && !devs.adb ? (
          <Group>
            <Row label={<span className="hstack"><Icon as={Wifi} />Android phones</span>} detail="Need Google's Android tools (downloaded once, with your agreement).">
              <Button disabled={!!busy} onClick={getTools}>Get Android Tools{"…"}</Button>
            </Row>
          </Group>
        ) : (
          <AndroidWifi phones={wifiAndroid} isOn={isOn} pick={pick} onChanged={refresh} />
        )}
      </div>

      <div>
        <button type="button" className="disclosure" aria-expanded={cable} onClick={() => setCable((c) => !c)}>
          <Icon as={ChevronRight} /><Icon as={Cable} />Use a cable for the best quality
        </button>
        {cable && (
          <>
            <Group>
              {devs?.ios.map((d) => {
                const s: PhoneSource = { kind: "ios", index: d.index, name: d.name };
                return (
                  <Row key={`i${d.index}`} label={<span className="hstack"><Icon as={Smartphone} />{d.name}</span>} detail="iPhone or iPad over USB">
                    <input type="radio" name="phone" aria-label={d.name} checked={isOn(s)} onChange={() => pick(s)} />
                  </Row>
                );
              })}
              {usbAndroid.map((d) => {
                const s: PhoneSource = { kind: "android", serial: d.serial, name: d.model };
                return (
                  <Row key={`a${d.serial}`} label={<span className="hstack"><Icon as={Smartphone} />{d.model}</span>}
                    detail={d.state === "device" ? "Android over USB" : "Allow USB debugging on the phone"}>
                    <input type="radio" name="phone" aria-label={d.model} disabled={d.state !== "device"} checked={isOn(s)} onChange={() => pick(s)} />
                  </Row>
                );
              })}
              {devs && devs.ios.length === 0 && usbAndroid.length === 0 && (
                <Row label="No phone on a cable" detail="iPhone: connect it, unlock it and tap Trust. Android: turn on USB debugging." />
              )}
            </Group>
            <div className="hstack after"><Button kind="plain" icon={RefreshCw} onClick={refresh}>Refresh</Button></div>
          </>
        )}
        <div className="after"><TaskStatus busy={busy || undefined} status={status} /></div>
      </div>

      <div>
        <p className="section-head" role="heading" aria-level={3}>Layout</p>
        <LayoutPicker panel={panel} onChange={layout} />
      </div>
    </>
  );
}

/** Android 11 and later: pair once with the code from Wireless debugging, then connect. */
function AndroidWifi({ phones, isOn, pick, onChanged }: {
  phones: PhoneDevices["android"]; isOn: (s: PhoneSource) => boolean; pick: (s: PhoneSource) => void; onChanged: () => void;
}) {
  const [pairAddr, setPairAddr] = useState("");
  const [code, setCode] = useState("");
  const [addr, setAddr] = useState("");
  const [busy, setBusy] = useState("");
  const [status, setStatus] = useState<Status>(null);
  const run = async (label: string, f: () => Promise<void>, done: string) => {
    setBusy(label); setStatus(null);
    try { await f(); setStatus({ text: done }); onChanged(); }
    catch (e) { setStatus({ text: errText(e), error: true }); }
    finally { setBusy(""); }
  };
  return (
    <>
      <Group>
        {phones.map((d) => {
          const s: PhoneSource = { kind: "android", serial: d.serial, name: d.model };
          return (
            <Row key={d.serial} label={<span className="hstack"><Icon as={Wifi} />{d.model}<StatusDot on /></span>} detail="Android over Wi-Fi">
              <input type="radio" name="phone" aria-label={d.model} disabled={d.state !== "device"} checked={isOn(s)} onChange={() => pick(s)} />
            </Row>
          );
        })}
        <Row block label="1. On the phone: Settings, Developer options, Wireless debugging, Pair device with pairing code.">
          <form className="hstack wrap" onSubmit={(e) => { e.preventDefault(); run("Pairing", () => api.androidWifiPair(pairAddr, code), "Paired. Now connect below."); }}>
            <input className="field" aria-label="Pairing address and port" placeholder="192.168.1.20:37099" value={pairAddr} onChange={(e) => setPairAddr(e.target.value)} />
            <input className="field num" aria-label="Pairing code" placeholder="6-digit code" inputMode="numeric" value={code} onChange={(e) => setCode(e.target.value)} />
            <Button type="submit" disabled={!!busy || !pairAddr || !code}>Pair</Button>
          </form>
        </Row>
        <Row block label="2. Then the address at the top of Wireless debugging.">
          <form className="hstack wrap" onSubmit={(e) => { e.preventDefault(); run("Connecting", () => api.androidWifiConnect(addr), "Connected. Pick the phone above."); }}>
            <input className="field" aria-label="Phone address and port" placeholder="192.168.1.20:41235" value={addr} onChange={(e) => setAddr(e.target.value)} />
            <Button type="submit" disabled={!!busy || !addr}>Connect</Button>
          </form>
        </Row>
      </Group>
      <div className="after"><TaskStatus busy={busy || undefined} status={status} /></div>
    </>
  );
}

/** A small 16:9 preview: drag the phone in picture-in-picture, or pick a side. */
export function LayoutPicker({ panel, onChange }: { panel: PhonePanel; onChange: (p: PhonePanel) => void }) {
  const box = useRef<HTMLDivElement>(null);
  const size = { S: 0.66, M: 0.8, L: 0.92 }[panel.size] * (panel.layout === "pip" ? 0.58 : 1);
  const drag = (e: React.PointerEvent) => {
    if (panel.layout !== "pip" || !box.current) return;
    const r = box.current.getBoundingClientRect();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => onChange({ ...panel, pip: [Math.min(0.95, Math.max(0.05, (ev.clientX - r.left) / r.width)), Math.min(0.95, Math.max(0.05, (ev.clientY - r.top) / r.height))] });
    const up = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up); };
    window.addEventListener("pointermove", move); window.addEventListener("pointerup", up);
  };
  const nudge = (e: React.KeyboardEvent) => {
    if (panel.layout !== "pip") return;
    const d = { ArrowLeft: [-0.03, 0], ArrowRight: [0.03, 0], ArrowUp: [0, -0.03], ArrowDown: [0, 0.03] }[e.key];
    if (d) { e.preventDefault(); onChange({ ...panel, pip: [Math.min(0.95, Math.max(0.05, panel.pip[0] + d[0])), Math.min(0.95, Math.max(0.05, panel.pip[1] + d[1]))] }); }
  };
  return (
    <div className="stack">
      <div className="hstack wrap">
        <Segmented label="Phone position" value={panel.layout} onChange={(l: PhoneLayout) => onChange({ ...panel, layout: l })} options={[
          { value: "right", label: "Right" }, { value: "left", label: "Left" }, { value: "pip", label: "Floating" }, { value: "split", label: "Split" },
        ]} />
        <Segmented label="Phone size" value={panel.size} onChange={(s: PhoneSize) => onChange({ ...panel, size: s })} options={[
          { value: "S", label: "S" }, { value: "M", label: "M" }, { value: "L", label: "L" },
        ]} />
      </div>
      <div ref={box} className={`layout-preview ${panel.layout}`} aria-label="Layout preview">
        <div className="laptop" />
        <div className="device" tabIndex={panel.layout === "pip" ? 0 : -1} role={panel.layout === "pip" ? "slider" : undefined}
          aria-label={panel.layout === "pip" ? "Phone position, arrow keys move it" : undefined}
          style={{ height: `${size * 100}%`, ...(panel.layout === "pip" ? { left: `${panel.pip[0] * 100}%`, top: `${panel.pip[1] * 100}%` } : {}) }}
          onPointerDown={drag} onKeyDown={nudge} />
      </div>
      {panel.layout === "pip" && <p className="callout secondary">Drag the phone to place it.</p>}
    </div>
  );
}
