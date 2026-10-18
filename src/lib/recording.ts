import { useEffect, useRef } from "react";
import { emit } from "@tauri-apps/api/event";
import { api, loadPrefs, type Prefs } from "./api";

/** Opens the floating setup pill (screen, mic, camera, record). */
export async function newRecording() {
  const p = loadPrefs();
  await api.hudLayout("setup");
  await api.overlays(true, !!p?.camera);
  await emit("bubble", { on: !!p?.camera });
  await emit("hud-setup");
}

/** Starts the countdown straight away with the last settings (global shortcut, tray). */
export async function beginRecording(p: Prefs) {
  await api.hudLayout("countdown");
  await api.overlays(true, p.camera);
  await emit("bubble", { on: p.camera });
  await emit("countdown", { screen: p.screen, mic: p.mic });
  await api.hideMain();
}

/** A menu command forwarded to the open screen; `n` changes on every press. */
export type Command = { name: string; n: number } | null;

/** Runs `on` for menu commands pressed after this screen mounted (not the one that opened it). */
export function useCommand(command: Command, on: (name: string) => void) {
  const seen = useRef(command?.n);
  const handler = useRef(on);
  handler.current = on;
  useEffect(() => {
    if (!command || command.n === seen.current) return;
    seen.current = command.n;
    handler.current(command.name);
  }, [command]);
}

