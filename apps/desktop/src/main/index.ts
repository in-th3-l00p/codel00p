import { app, BrowserWindow, ipcMain, shell } from "electron";
import { join } from "node:path";

import { engineStatus, localSessions } from "./local-engine";
import { signInWithBrowser } from "./auth-bridge";

ipcMain.handle("local:sessions", () => localSessions());
ipcMain.handle("local:engine-status", () => engineStatus());
ipcMain.handle("auth:sign-in-with-browser", () => signInWithBrowser());
ipcMain.handle("shell:open-external", (_event, url: string) => {
  if (typeof url === "string" && /^https?:\/\//.test(url)) {
    return shell.openExternal(url);
  }
  return Promise.resolve();
});

const createWindow = () => {
  const window = new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 960,
    minHeight: 640,
    show: false,
    backgroundColor: "#0c0a10",
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "default",
    trafficLightPosition:
      process.platform === "darwin" ? { x: 18, y: 18 } : undefined,
    webPreferences: {
      preload: join(__dirname, "../preload/index.js")
    }
  });

  window.once("ready-to-show", () => window.show());

  if (process.env.ELECTRON_RENDERER_URL) {
    window.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    window.loadFile(join(__dirname, "../renderer/index.html"));
  }
};

app.whenReady().then(() => {
  createWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});
