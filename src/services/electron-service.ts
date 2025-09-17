/**
 * Direct Electron service for renderer process when using nodeIntegration mode
 */
export class ElectronService {
  private static getIpcRenderer() {
    try {
      // Use dynamic import to avoid webpack issues in development
      const electron = window.require("electron");
      return electron.ipcRenderer;
    } catch (error) {
      console.error("Error accessing ipcRenderer:", error);
      return null;
    }
  }

  /** Send log to file */
  public static async logToFile(level: string, message: string): Promise<void> {
    const ipcRenderer = this.getIpcRenderer();
    if (!ipcRenderer) return;
    ipcRenderer.send("log-to-file", level, message);
  }

  /**
   * Open directory dialog
   */
  public static async openDirectoryDialog(): Promise<string> {
    const ipcRenderer = this.getIpcRenderer();
    if (!ipcRenderer) return "";
    return ipcRenderer.invoke("open-directory-dialog");
  }

  /**
   * Open save dialog
   */
  public static async openSaveDialog(): Promise<string> {
    const ipcRenderer = this.getIpcRenderer();
    if (!ipcRenderer) return "";
    return ipcRenderer.invoke("open-save-dialog");
  }

  /**
   * Start recording bridge
   */
  public static async startRecordingBridge(
    startKey: string,
    stopKey: string,
  ): Promise<boolean> {
    const ipcRenderer = this.getIpcRenderer();
    if (!ipcRenderer) return false;
    return ipcRenderer.invoke("start-recording-bridge", startKey, stopKey);
  }

  /**
   * Start upload bridge
   */
  public static async startUploadBridge(apiToken: string): Promise<boolean> {
    const ipcRenderer = this.getIpcRenderer();
    if (!ipcRenderer) return false;
    return ipcRenderer.invoke("start-upload-bridge", apiToken);
  }

  /**
   * Save credentials
   */
  public static async saveCredentials(
    key: string,
    value: string,
  ): Promise<{ success: boolean; error?: string }> {
    const ipcRenderer = this.getIpcRenderer();
    if (!ipcRenderer) return { success: false, error: "IPC not available" };
    return ipcRenderer.invoke("save-credentials", key, value);
  }

  /**
   * Load credentials
   */
  public static async loadCredentials(): Promise<{
    success: boolean;
    data: any;
    error?: string;
  }> {
    const ipcRenderer = this.getIpcRenderer();
    if (!ipcRenderer)
      return { success: false, data: {}, error: "IPC not available" };
    return ipcRenderer.invoke("load-credentials");
  }

  /**
   * Save preferences
   */
  public static async savePreferences(
    preferences: any,
  ): Promise<{ success: boolean; error?: string }> {
    const ipcRenderer = this.getIpcRenderer();
    if (!ipcRenderer) return { success: false, error: "IPC not available" };
    return ipcRenderer.invoke("save-preferences", preferences);
  }

  /**
   * Load preferences
   */
  public static async loadPreferences(): Promise<{
    success: boolean;
    data: any;
    error?: string;
  }> {
    const ipcRenderer = this.getIpcRenderer();
    if (!ipcRenderer)
      return {
        success: false,
        data: { uploadFrequency: "one", showRecordButton: true },
        error: "IPC not available",
      };
    return ipcRenderer.invoke("load-preferences");
  }
}
