import { contextBridge, ipcRenderer } from "electron";

// Expose protected methods that allow the renderer process to use
// the ipcRenderer without exposing the entire object
contextBridge.exposeInMainWorld("api", {
  // Dialog handling
  openDirectoryDialog: () => ipcRenderer.invoke("open-directory-dialog"),
  openSaveDialog: () => ipcRenderer.invoke("open-save-dialog"),

  // Recording control
  startRecording: (recordingPath: string, outputPath: string) =>
    ipcRenderer.invoke("start-recording", recordingPath, outputPath),
  stopRecording: () => ipcRenderer.invoke("stop-recording"),
});
