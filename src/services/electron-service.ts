import { Credentials, Preferences, unwrapIpcResponse } from "@/settings";

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
      throw error;
    }
  }

  /** Send log to file */
  public static async logToFile(level: string, message: string): Promise<void> {
    const ipcRenderer = this.getIpcRenderer();
    ipcRenderer.send("log-to-file", level, message);
  }

  /**
   * Open directory dialog
   */
  public static async openDirectoryDialog(): Promise<string> {
    const ipcRenderer = this.getIpcRenderer();
    return ipcRenderer.invoke("open-directory-dialog");
  }

  /**
   * Open save dialog
   */
  public static async openSaveDialog(): Promise<string> {
    const ipcRenderer = this.getIpcRenderer();
    return ipcRenderer.invoke("open-save-dialog");
  }

  /**
   * Save credentials
   */
  public static async saveCredentials(
    credentials: Partial<Credentials>,
  ): Promise<void> {
    const ipcRenderer = this.getIpcRenderer();
    unwrapIpcResponse<undefined>(
      await ipcRenderer.invoke("save-credentials", credentials),
    );
  }

  /**
   * Load credentials
   */
  public static async loadCredentials(): Promise<Credentials> {
    const ipcRenderer = this.getIpcRenderer();
    return unwrapIpcResponse<Credentials>(
      await ipcRenderer.invoke("load-credentials"),
    );
  }

  /**
   * Save preferences
   */
  public static async savePreferences(
    preferences: Partial<Preferences>,
  ): Promise<void> {
    const ipcRenderer = this.getIpcRenderer();
    unwrapIpcResponse<undefined>(
      await ipcRenderer.invoke("save-preferences", preferences),
    );
  }

  /**
   * Load preferences
   */
  public static async loadPreferences(): Promise<Preferences> {
    const ipcRenderer = this.getIpcRenderer();
    return unwrapIpcResponse<Preferences>(
      await ipcRenderer.invoke("load-preferences"),
    );
  }

  /*
   * Close settings window
   */
  public static async closeSettingsWindow(): Promise<void> {
    const ipcRenderer = this.getIpcRenderer();
    await ipcRenderer.invoke("close-settings");
  }

  /*
   * Authentication completed
   */
  public static async authenticationCompleted(): Promise<void> {
    const ipcRenderer = this.getIpcRenderer();
    await ipcRenderer.invoke("authentication-completed");
  }

  /*
   * Resize window for consent page
   */
  public static async resizeForConsent(): Promise<void> {
    const ipcRenderer = this.getIpcRenderer();
    await ipcRenderer.invoke("resize-for-consent");
  }

  /*
   * Resize window for API key page
   */
  public static async resizeForApiKey(): Promise<void> {
    const ipcRenderer = this.getIpcRenderer();
    await ipcRenderer.invoke("resize-for-api-key");
  }
}
