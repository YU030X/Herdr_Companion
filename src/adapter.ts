import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { CompanionSnapshot, ConnectionView, RuntimeView } from "./model";

const SNAPSHOT_EVENT = "companion://snapshot-changed";
const CONNECTION_EVENT = "companion://connection-changed";

export function getAppState(): Promise<RuntimeView> {
  return invoke<RuntimeView>("get_app_state");
}

export function retryConnection(): Promise<void> {
  return invoke("retry_connection");
}

export function setAlwaysOnTop(enabled: boolean): Promise<void> {
  return invoke("set_always_on_top", { enabled });
}

export function minimizeWindow(): Promise<void> {
  return getCurrentWindow().minimize();
}

export function closeWindow(): Promise<void> {
  return getCurrentWindow().close();
}

export async function subscribeToRuntime(
  onSnapshot: (snapshot: CompanionSnapshot) => void,
  onConnection: (connection: ConnectionView) => void,
): Promise<UnlistenFn> {
  const registrations = await Promise.allSettled([
    listen<CompanionSnapshot>(SNAPSHOT_EVENT, (event) => onSnapshot(event.payload)),
    listen<ConnectionView>(CONNECTION_EVENT, (event) => onConnection(event.payload)),
  ]);
  const unlisten = registrations.flatMap((result) => result.status === "fulfilled" ? [result.value] : []);
  const failure = registrations.find((result) => result.status === "rejected");
  if (failure?.status === "rejected") {
    for (const stop of unlisten) stop();
    throw failure.reason;
  }

  return () => {
    for (const stop of unlisten) stop();
  };
}
