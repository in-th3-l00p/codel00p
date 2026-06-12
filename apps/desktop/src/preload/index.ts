import { contextBridge, ipcRenderer } from "electron";

import type { EngineStatus, LocalSessionsResult } from "../main/local-engine";
import type { BrowserSignInResult } from "../main/auth-bridge";

contextBridge.exposeInMainWorld("codel00p", {
  platform: process.platform,
  local: {
    sessions: (): Promise<LocalSessionsResult> =>
      ipcRenderer.invoke("local:sessions"),
    engineStatus: (): Promise<EngineStatus> =>
      ipcRenderer.invoke("local:engine-status")
  },
  auth: {
    signInWithBrowser: (): Promise<BrowserSignInResult> =>
      ipcRenderer.invoke("auth:sign-in-with-browser")
  },
  openExternal: (url: string): Promise<void> =>
    ipcRenderer.invoke("shell:open-external", url)
});
