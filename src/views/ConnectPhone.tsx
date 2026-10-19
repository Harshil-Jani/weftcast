import { useCallback, useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ask } from "@tauri-apps/plugin-dialog";
import { Cable, RefreshCw, Smartphone, Wifi } from "lucide-react";
import { api, brand, DEFAULT_PANEL, errText, loadPrefs, savePrefs, type PairRole, type PhoneDevices, type PhoneLayout, type PhonePanel, type PhoneSize, type PhoneSource } from "../lib/api";
import { pairState } from "../lib/phoneLink";
import { Button, Group, Icon, Row, Segmented, Sheet, TaskStatus, Toggle, type Status } from "../components/ui";

type Mode = "screen" | "camera" | "angle";

/** Connect a phone: its screen over USB, or its camera over Wi-Fi as the face cam or a second angle (with a remote). */
export default function ConnectPhone({ onClose }: { onClose: () => void }) {
  const [mode, setMode] = useState<Mode>("screen");
  return (
    <Sheet title="Connect Phone" wide onClose={onClose} footer={<Button large kind="primary" onClick={onClose}>Done</Button>}>
      <Segmented label="What the phone does" full value={mode} onChange={setMode} options={[
        { value: "screen", label: "Phone Screen" }, { value: "camera", label: "Face Cam" }, { value: "angle", label: "Remote & Angle" },
      ]} />
      {mode === "screen" ? <ScreenMode /> : <QrMode key={mode} role={mode} />}
    </Sheet>
  );
}

function ScreenMode() {
  const [devs, setDevs] = useState<PhoneDevices | null>(null);
  const [status, setStatus] = useState<Status>(null);
  const [busy, setBusy] = useState("");
  const [source, setSource] = useState<PhoneSource | null>(() => loadPrefs()?.phone ?? null);
  const [panel, setPanel] = useState<PhonePanel>(() => loadPrefs()?.phone?.layout ?? DEFAULT_PANEL);
  const refresh = useCallback(() => { api.phoneDevices().then(setDevs).catch((e) => setStatus({ text: errText(e), error: true })); }, []);
  useEffect(refresh, [refresh]);
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
  const isOn = (s: PhoneSource) => source?.kind === s.kind && (s.kind === "ios" ? source.kind === "ios" && source.index === s.index : source.kind === "android" && source.serial === s.serial);

  return (
    <>
      <p className="callout secondary">Record the phone's screen next to yours, for app demos. It is saved as its own file, so the layout can change after recording.</p>
      <div>
        <h3 className="section-head">USB Devices</h3>
        <Group>
          <Row label={<span className="hstack"><Icon as={Cable} />No phone screen</span>}>
            <input type="radio" name="phone" aria-label="No phone screen" checked={!source} onChange={() => pick(null)} />
          </Row>
          {devs?.ios.map((d) => {
            const s: PhoneSource = { kind: "ios", index: d.index, name: d.name };
            return (
              <Row key={`i${d.index}`} label={<span className="hstack"><Icon as={Smartphone} />{d.name}</span>} detail="iPhone or iPad over USB">
                <input type="radio" name="phone" aria-label={d.name} checked={isOn(s)} onChange={() => pick(s)} />
              </Row>
            );
          })}
          {devs?.android.map((d) => {
            const s: PhoneSource = { kind: "android", serial: d.serial, name: d.model };
            return (
              <Row key={`a${d.serial}`} label={<span className="hstack"><Icon as={Smartphone} />{d.model}</span>}
                detail={d.state === "device" ? "Android over USB" : "Allow USB debugging on the phone"}>
                <input type="radio" name="phone" aria-label={d.model} disabled={d.state !== "device"} checked={isOn(s)} onChange={() => pick(s)} />
              </Row>
            );
          })}
          {devs && !devs.adb && (
            <Row label="Android phones" detail="Need Google's Android tools (downloaded once, with your agreement).">
              <Button disabled={!!busy} onClick={getTools}>Get Android Tools{"…"}</Button>
            </Row>
          )}
        </Group>
        <div className="hstack after">
          <Button kind="plain" icon={RefreshCw} onClick={refresh}>Refresh</Button>
          <span className="callout secondary">iPhone: connect the cable, unlock it and tap Trust. Android: turn on USB debugging.</span>
        </div>
        <div className="after"><TaskStatus busy={busy || undefined} status={status} /></div>
      </div>
      <div>
        <h3 className="section-head">Layout</h3>
        <LayoutPicker panel={panel} onChange={layout} />
      </div>
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

function QrMode({ role }: { role: PairRole }) {
  const [pub, setPub] = useState(false);
  const [code, setCode] = useState<{ url: string; svg: string; lan: boolean } | null>(null);
  const [status, setStatus] = useState<Status>(null);
  const [device, setDevice] = useState<string | null>(null);
  const [cams, setCams] = useState<{ id: string; label: string }[]>([]);
  const [cameraId, setCameraId] = useState<string | null>(() => loadPrefs()?.cameraId ?? null);

  useEffect(() => {
    setCode(null); setStatus(null);
    api.phonePair(role, pub).then(setCode).catch((e) => setStatus({ text: errText(e), error: true }));
  }, [role, pub]);
  useEffect(() => {
    const t = window.setInterval(() => pairState().then((p) => setDevice(p.paired && p.connected ? p.device || "Phone" : null)).catch(() => {}), 1000);
    return () => clearInterval(t);
  }, []);
  // Cameras this Mac can use directly, including an iPhone as a Continuity Camera.
  useEffect(() => {
    if (role !== "camera") return;
    navigator.mediaDevices?.enumerateDevices().then((d) => setCams(d.filter((x) => x.kind === "videoinput").map((x, i) => ({ id: x.deviceId, label: x.label || `Camera ${i + 1}` })))).catch(() => {});
  }, [role]);
  const chooseCam = (id: string | null) => {
    setCameraId(id);
    const p = loadPrefs();
    savePrefs({ ...(p ?? { screen: "", mic: null, camera: true }), cameraId: id });
  };

  return (
    <>
      <p className="callout secondary">
        {role === "camera"
          ? "Use your phone as the face cam: it shows in the camera bubble and is also recorded as its own track."
          : "Use your phone as a remote (start, pause, stop) and as a second angle for hands, a whiteboard or a product."}
      </p>
      <div className="pair">
        <div className="qr" role="img" aria-label="QR code to open on your phone">
          {code ? <span dangerouslySetInnerHTML={{ __html: code.svg }} /> : <span className="spinner" />}
        </div>
        <div className="stack">
          <h3 className="headline">Scan with your phone's camera</h3>
          <p className="callout secondary">
            {code?.lan === false
              ? "Opens through a temporary public address. The video still goes straight to this Mac when it can."
              : "Your phone must be on the same Wi-Fi. The page uses this Mac's own certificate, so the phone asks you to confirm it once."}
          </p>
          {code && <button type="button" className="link callout truncate" onClick={() => writeText(code.url)} title="Copy link">{code.url.split("#")[0]}</button>}
          <p className={`callout ${device ? "ok" : "secondary"}`} role="status">
            <Icon as={device ? Smartphone : Wifi} size={14} /> {device ? `Phone connected: ${device}` : "Waiting for your phone"}
          </p>
          <TaskStatus status={status} />
        </div>
      </div>
      <Group>
        <Row label="Use a public link instead" detail="For guest Wi-Fi that keeps devices apart. Off keeps everything on your network.">
          <Toggle label="Use a public link instead" checked={pub} onChange={setPub} />
        </Row>
      </Group>
      {role === "camera" && cams.length > 0 && (
        <div>
          <h3 className="section-head">Or a Camera on This Mac</h3>
          <Group>
            <Row label="Built-in default"><input type="radio" name="cam" aria-label="Built-in default camera" checked={!cameraId} onChange={() => chooseCam(null)} /></Row>
            {cams.map((c) => (
              <Row key={c.id} label={c.label} detail={/iphone|ipad/i.test(c.label) ? "Continuity Camera" : undefined}>
                <input type="radio" name="cam" aria-label={c.label} checked={cameraId === c.id} onChange={() => chooseCam(c.id)} />
              </Row>
            ))}
          </Group>
        </div>
      )}
    </>
  );
}
