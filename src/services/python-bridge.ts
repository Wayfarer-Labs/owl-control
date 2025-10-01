import { ElectronService } from "./electron-service";

/**
 * Interface for app preferences
 */
export interface AppPreferences {
  startRecordingKey?: string;
  stopRecordingKey?: string;
  apiToken?: string;
}

/**
 * Bridge to Python backend
 */
export class PythonBridge {
  private preferences: AppPreferences = {
    startRecordingKey: "f4",
    stopRecordingKey: "f5",
    apiToken: "",
  };

  constructor() {
    this.loadPreferences();
  }

  /**
   * Load preferences from storage
   */
  public loadPreferences(): AppPreferences {
    try {
      // Try to load from Electron's secure storage
      ElectronService.loadPreferences()
        .then((result) => {
          if (result.success && result.data) {
            this.preferences = {
              ...this.preferences,
              ...result.data,
            };
            // Ensure hotkeys have default values if not set
            if (!this.preferences.startRecordingKey) {
              this.preferences.startRecordingKey = "f4";
            }
            if (!this.preferences.stopRecordingKey) {
              this.preferences.stopRecordingKey = "f5";
            }
          }
        })
        .catch((error) => {
          console.error("Error loading preferences from Electron:", error);
        });
    } catch (error) {
      console.error("Error loading preferences:", error);
    }

    // Ensure hotkeys have default values if not set
    if (!this.preferences.startRecordingKey) {
      this.preferences.startRecordingKey = "f4";
    }
    if (!this.preferences.stopRecordingKey) {
      this.preferences.stopRecordingKey = "f5";
    }

    return this.preferences;
  }

  /**
   * Save preferences to storage
   */
  public async savePreferences(preferences: AppPreferences): Promise<boolean> {
    try {
      this.preferences = {
        ...this.preferences,
        ...preferences,
      };

      // Save to Electron's secure storage
      await ElectronService.savePreferences(this.preferences);

      return true;
    } catch (error) {
      console.error("Error saving preferences:", error);
      return false;
    }
  }

  /**
   * Start recording bridge
   */
  public async startRecordingBridge(): Promise<boolean> {
    try {
      if (
        !this.preferences.startRecordingKey ||
        !this.preferences.stopRecordingKey
      ) {
        console.error("Start recording key or stop recording key not set");
        return false;
      }

      // Call Electron service to start Python recording bridge process
      return await ElectronService.startRecordingBridge(
        this.preferences.startRecordingKey,
        this.preferences.stopRecordingKey,
      );
    } catch (error) {
      console.error("Error starting recording bridge:", error);
      return false;
    }
  }
}
