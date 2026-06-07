import { contextBridge } from "electron";

contextBridge.exposeInMainWorld("codel00p", {
  platform: process.platform
});
