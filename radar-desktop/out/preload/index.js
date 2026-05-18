"use strict";
const electron = require("electron");
const driftApi = {
  getApiUrl: () => electron.ipcRenderer.invoke("get-api-url")
};
electron.contextBridge.exposeInMainWorld("drift", driftApi);
