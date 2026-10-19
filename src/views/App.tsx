import { useCallback, useEffect, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { Menu, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ask } from "@tauri-apps/plugin-dialog";
import { Clapperboard, Film, Link2, Settings as Gear, Smartphone, Video as VideoIcon } from "lucide-react";
import { api, base, brand, loadPrefs, mmss, type Video } from "../lib/api";
import { beginRecording, newRecording, type Command } from "../lib/recording";
import { Button, Icon, SidebarItem } from "../components/ui";
import { VideoCard } from "../components/VideoCard";
import Player from "./Player";
import Settings from "./Settings";
import Onboarding from "./Onboarding";
import Shorts from "./Shorts";
import Shared from "./Shared";
import ConnectPhone from "./ConnectPhone";

type View = "library" | "shorts" | "shared";
const stored = (k: string, d: boolean) => { try { const v = localStorage.getItem(k); return v === null ? d : v === "1"; } catch { return d; } };
const store = (k: string, v: boolean) => { try { localStorage.setItem(k, v ? "1" : "0"); } catch { /* not persisted */ } };

export default function App() {
  const [view, setView] = useState<View>("library");
  const [videos, setVideos] = useState<Video[] | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [srv, setSrv] = useState("");
  const [sheet, setSheet] = useState<"settings" | "onboarding" | "phone" | null>(null);
  const [sidebar, setSidebar] = useState(() => stored("sidebar", true));
  const [command, setCommand] = useState<Command>(null);
  const n = useRef(0);

  const refresh = useCallback(() => { api.list().then(setVideos); }, []);
  // Navigating drops any pending menu command, so a screen opened later never replays an old one.
  const go = useCallback((v: View) => { setView(v); setOpen(null); setCommand(null); refresh(); }, [refresh]);

  useEffect(() => {
    refresh();
    base().then(setSrv);
    api.settings().then((s) => { if (!s.onboarded) setSheet("onboarding"); });
    const offs = [
      listen<string>("recording-saved", (e) => { refresh(); setView("library"); setOpen(e.payload); }),
      listen("toggle-recording", async () => {
        const s = await api.state();
        if (s.recording) { await emit("bubble", { on: false }); await api.stop(); return; }
        const p = loadPrefs();
        if (p) await beginRecording(p); else await newRecording();
      }),
      listen("connect-phone", () => setSheet("phone")),
      listen<string>("menu", (e) => {
        const id = e.payload;
        if (id === "settings") setSheet("settings");
        else if (id === "connect-phone") setSheet("phone");
        else if (id === "new-recording") newRecording();
        else if (id === "view-library") go("library");
        else if (id === "view-shorts") go("shorts");
        else if (id === "view-shared") go("shared");
        else if (id === "toggle-sidebar") setSidebar((s) => { store("sidebar", !s); return !s; });
        else {
          if (id === "import") { setView("shorts"); setOpen(null); }
          setCommand({ name: id, n: ++n.current });
        }
      }),
    ];
    return () => { offs.forEach((o) => o.then((f) => f())); };
  }, [refresh, go]);

  const contextMenu = async (v: Video) => {
    const pub = await api.shareStatus();
    const link = pub ? `${pub}/v/${v.share_id}` : `${srv}/v/${v.share_id}`;
    const menu = await Menu.new({
      items: [
        { id: "open", text: "Open", action: () => setOpen(v.id) },
        { id: "copy", text: pub ? "Copy Public Link" : "Copy Local Link", action: () => writeText(link) },
        { id: "finder", text: "Show in Finder", action: () => api.reveal(v.id) },
        { id: "shorts", text: "Make Shorts…", action: () => { setView("shorts"); setCommand({ name: `shorts:${v.id}`, n: ++n.current }); } },
        await PredefinedMenuItem.new({ item: "Separator" }),
        { id: "delete", text: "Delete…", action: async () => {
          if (await ask(`"${v.title}" and its transcript will be removed from this Mac. This can't be undone.`, { title: "Delete this recording?", kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" })) {
            await api.remove(v.id); refresh();
          }
        } },
      ],
    });
    await menu.popup();
  };

  const list = videos ?? [];
  return (
    <div className={`window ${sidebar ? "" : "no-sidebar"}`}>
      <aside className="sidebar" data-tauri-drag-region aria-label="Sidebar">
        <SidebarItem icon={Film} label="Library" current={view === "library" && !open} count={videos?.length} onClick={() => go("library")} />
        <SidebarItem icon={Smartphone} label="Shorts" current={view === "shorts"} onClick={() => go("shorts")} />
        <SidebarItem icon={Link2} label="Shared" current={view === "shared"} onClick={() => go("shared")} />
        <div className="foot">
          <SidebarItem icon={Gear} label="Settings" current={sheet === "settings"} onClick={() => setSheet("settings")} />
        </div>
      </aside>

      <main className="main">
        {view === "shorts" ? (
          <Shorts videos={list} srv={srv} onImported={refresh} command={command} />
        ) : view === "shared" ? (
          <Shared videos={list} srv={srv} onOpen={(id) => { setView("library"); setOpen(id); }} />
        ) : open ? (
          <Player key={open} id={open} command={command} onBack={() => { setOpen(null); refresh(); }}
            onDeleted={() => { setOpen(null); refresh(); }} onOpen={(id) => { refresh(); setOpen(id); }} />
        ) : (
          <Library videos={videos} srv={srv} onOpen={setOpen} onMenu={contextMenu} />
        )}
      </main>

      {sheet === "settings" && <Settings onClose={() => setSheet(null)} />}
      {sheet === "onboarding" && <Onboarding onDone={() => setSheet(null)} />}
      {sheet === "phone" && <ConnectPhone onClose={() => setSheet(null)} />}
    </div>
  );
}

function Library({ videos, srv, onOpen, onMenu }: { videos: Video[] | null; srv: string; onOpen: (id: string) => void; onMenu: (v: Video) => void }) {
  const total = videos?.reduce((s, v) => s + v.duration, 0) ?? 0;
  return (
    <>
      <div className="toolbar" data-tauri-drag-region>
        <h1 className="title" data-tauri-drag-region>Library</h1>
        {videos && videos.length > 0 && <span className="subtitle num" data-tauri-drag-region>{videos.length} {videos.length === 1 ? "video" : "videos"}, {mmss(total)}</span>}
        <span className="spacer" data-tauri-drag-region />
        <Button kind="primary" icon={VideoIcon} onClick={newRecording} title="New Recording (Command N)">New Recording</Button>
      </div>
      <div className="page">
        {videos === null ? (
          <div className="lib-grid" aria-busy="true" aria-label="Loading library">
            {Array.from({ length: 6 }, (_, i) => <div key={i} className="stack"><div className="skeleton" style={{ aspectRatio: "16 / 10" }} /><div className="skeleton" style={{ height: 14, width: "70%" }} /></div>)}
          </div>
        ) : videos.length === 0 ? (
          <div className="empty">
            <Icon as={Clapperboard} size={44} />
            <h2 className="title2">No recordings yet</h2>
            <p>{brand.tagline}</p>
            <Button kind="primary" large icon={VideoIcon} onClick={newRecording}>New Recording</Button>
            <p className="caption">Press <kbd>⌘N</kbd> here, or <kbd>⌥⇧R</kbd> in any app to start and stop.</p>
          </div>
        ) : (
          <div className="lib-grid">
            {videos.map((v) => <VideoCard key={v.id} video={v} srv={srv} onOpen={() => onOpen(v.id)} onMenu={() => onMenu(v)} />)}
          </div>
        )}
      </div>
    </>
  );
}
