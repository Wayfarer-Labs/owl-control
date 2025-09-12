import { ElectronService } from './electron-service';

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
    startRecordingKey: 'f4',
    stopRecordingKey: 'f5',
    apiToken: '',
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
        .then(result => {
          if (result.success && result.data) {
            this.preferences = {
              ...this.preferences,
              ...result.data
            };
            // Ensure hotkeys have default values if not set
            if (!this.preferences.startRecordingKey) {
              this.preferences.startRecordingKey = 'f4';
            }
            if (!this.preferences.stopRecordingKey) {
              this.preferences.stopRecordingKey = 'f5';
            }
          }
        })
        .catch(error => {
          console.error('Error loading preferences from Electron:', error);
        });
      
      // For immediate return, check localStorage
      try {
        const storedPrefs = localStorage.getItem('appPreferences');
        if (storedPrefs) {
          const parsed = JSON.parse(storedPrefs);
          this.preferences = {
            ...this.preferences,
            ...parsed
          };
        }
      } catch (error) {
        console.error('Error loading preferences from localStorage:', error);
      }
    } catch (error) {
      console.error('Error loading preferences:', error);
    }
    
    // Ensure hotkeys have default values if not set
    if (!this.preferences.startRecordingKey) {
      this.preferences.startRecordingKey = 'f4';
    }
    if (!this.preferences.stopRecordingKey) {
      this.preferences.stopRecordingKey = 'f5';
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
        ...preferences
      };
      
      // Save to Electron's secure storage
      await ElectronService.savePreferences(this.preferences);
      
      // Also save to localStorage as fallback
      try {
        localStorage.setItem('appPreferences', JSON.stringify(this.preferences));
      } catch (error) {
        console.error('Error saving preferences to localStorage:', error);
      }
      
      return true;
    } catch (error) {
      console.error('Error saving preferences:', error);
      return false;
    }
  }

  /**
   * Start recording bridge
   */
  public async startRecordingBridge(): Promise<boolean> {
    try {
      // Call Electron service to start Python recording bridge process
      return await ElectronService.startRecordingBridge(this.preferences.startRecordingKey, this.preferences.stopRecordingKey);
    } catch (error) {
      console.error('Error starting recording bridge:', error);
      return false;
    }
  }

  /**
   * Start upload bridge
   */
  public async startUploadBridge(): Promise<boolean> {
    try {
      // Call Electron service to start Python upload bridge process
      return await ElectronService.startUploadBridge(this.preferences.apiToken || '');
    } catch (error) {
      console.error('Error starting upload bridge:', error);
      return false;
    }
  }
}