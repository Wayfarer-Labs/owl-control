import { ipcRenderer } from 'electron';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

export interface UploadProgress {
  totalFiles: number;
  uploadedFiles: number;
  currentFile: string;
  bytesUploaded: number;
  totalBytes: number;
  speed: string;
  eta: string;
  isUploading: boolean;
}

export interface UploadStats {
  totalDurationUploaded: number; // in seconds
  totalFilesUploaded: number;
  totalVolumeUploaded: number; // in bytes
  lastUploadDate: string;
}

export class UploadService {
  private static instance: UploadService;
  private uploadProcess: any = null;
  private statsFilePath: string;
  private progressFilePath: string;
  private progressPollingInterval: NodeJS.Timeout | null = null;

  constructor() {
    // Store stats in temp directory
    this.statsFilePath = path.join(os.tmpdir(), 'owl-control-upload-stats.json');
    this.progressFilePath = path.join(os.tmpdir(), 'owl-control-upload-progress.json');
  }

  public static getInstance(): UploadService {
    if (!UploadService.instance) {
      UploadService.instance = new UploadService();
    }
    return UploadService.instance;
  }

  /**
   * Get upload statistics from temp file
   */
  public getUploadStats(): UploadStats {
    try {
      if (fs.existsSync(this.statsFilePath)) {
        const data = fs.readFileSync(this.statsFilePath, 'utf-8');
        return JSON.parse(data);
      }
    } catch (error) {
      console.error('Error reading upload stats:', error);
    }

    // Return default stats
    return {
      totalDurationUploaded: 0,
      totalFilesUploaded: 0,
      totalVolumeUploaded: 0,
      lastUploadDate: 'Never'
    };
  }

  /**
   * Update upload statistics
   */
  private updateUploadStats(additionalDuration: number, additionalFiles: number, additionalVolume: number = 0) {
    try {
      const currentStats = this.getUploadStats();
      const newStats: UploadStats = {
        totalDurationUploaded: currentStats.totalDurationUploaded + additionalDuration,
        totalFilesUploaded: currentStats.totalFilesUploaded + additionalFiles,
        totalVolumeUploaded: currentStats.totalVolumeUploaded + additionalVolume,
        lastUploadDate: new Date().toISOString()
      };

      fs.writeFileSync(this.statsFilePath, JSON.stringify(newStats, null, 2));
    } catch (error) {
      console.error('Error updating upload stats:', error);
    }
  }

  /**
   * Start upload process with progress tracking
   */
  public async startUpload(
    apiToken: string, 
    progressCallback?: (progress: UploadProgress) => void
  ): Promise<{ success: boolean; message?: string; stats?: UploadStats }> {
    if (this.uploadProcess) {
      return { success: false, message: 'Upload already in progress' };
    }

    try {
      // Start the Python upload process with special flags for progress output
      const result = await ipcRenderer.invoke('start-upload-with-progress', {
        apiToken,
        progressOutput: true
      });

      if (result.success && result.processId) {
        this.uploadProcess = result.processId;
        
        // Listen for progress updates
        if (progressCallback) {
          this.listenForProgress(progressCallback);
        }

        return { success: true, message: 'Upload started successfully' };
      } else {
        return { success: false, message: result.error || 'Failed to start upload' };
      }
    } catch (error) {
      console.error('Error starting upload:', error);
      return { success: false, message: 'Failed to start upload process' };
    }
  }

  /**
   * Stop current upload process
   */
  public async stopUpload(): Promise<{ success: boolean; message?: string }> {
    if (!this.uploadProcess) {
      return { success: false, message: 'No upload in progress' };
    }

    try {
      await ipcRenderer.invoke('stop-upload-process', this.uploadProcess);
      this.uploadProcess = null;
      return { success: true, message: 'Upload stopped' };
    } catch (error) {
      console.error('Error stopping upload:', error);
      return { success: false, message: 'Failed to stop upload' };
    }
  }

  /**
   * Check if upload is currently in progress
   */
  public isUploading(): boolean {
    return this.uploadProcess !== null;
  }

  /**
   * Start polling the progress file for upload updates
   */
  private startProgressPolling(callback: (progress: UploadProgress) => void) {
    // Track progress state across updates
    let progressState = {
      totalFiles: 0,
      uploadedFiles: 0,
      currentFile: '',
      bytesUploaded: 0,
      totalBytes: 0,
      speed: '0 MB/s',
      eta: 'Calculating...',
      isUploading: true
    };

    this.progressPollingInterval = setInterval(() => {
      try {
        if (fs.existsSync(this.progressFilePath)) {
          const progressData = JSON.parse(fs.readFileSync(this.progressFilePath, 'utf-8'));
          
          // Update progress state with file data
          if (progressData.phase === 'upload') {
            progressState.bytesUploaded = progressData.bytes_uploaded || 0;
            progressState.totalBytes = progressData.total_bytes || 0;
            
            // Format speed
            const speedMbps = progressData.speed_mbps || 0;
            progressState.speed = speedMbps > 0 ? `${speedMbps.toFixed(1)} MB/s` : '0 MB/s';
            
            // Format ETA
            const etaSeconds = progressData.eta_seconds || 0;
            if (etaSeconds > 0 && etaSeconds < 3600) {
              const minutes = Math.floor(etaSeconds / 60);
              const seconds = Math.floor(etaSeconds % 60);
              progressState.eta = minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
            } else {
              progressState.eta = etaSeconds > 0 ? 'Calculating...' : 'Complete';
            }
            
            // Update current file status
            const percent = Math.round(progressData.percent || 0);
            progressState.currentFile = progressData.action === 'complete' 
              ? 'Upload complete!' 
              : `Uploading... ${percent}%`;
            
            // Set file progress to 1 of 1 when uploading
            progressState.totalFiles = 1;
            progressState.uploadedFiles = progressData.action === 'complete' ? 1 : 0;
          }
          
          callback({ ...progressState });
        }
      } catch (error) {
        console.error('Error reading progress file:', error);
      }
    }, 500); // Poll every 500ms
  }

  /**
   * Stop polling the progress file
   */
  private stopProgressPolling() {
    if (this.progressPollingInterval) {
      clearInterval(this.progressPollingInterval);
      this.progressPollingInterval = null;
    }
  }

  /**
   * Listen for progress updates from the upload process
   */
  private listenForProgress(callback: (progress: UploadProgress) => void) {
    // Start polling the progress file
    this.startProgressPolling(callback);

    // Still listen for upload completion from IPC
    ipcRenderer.on('upload-complete', (_, result) => {
      this.uploadProcess = null;
      this.stopProgressPolling();
      
      // Update stats if upload was successful
      if (result.success) {
        this.updateUploadStats(
          result.totalDuration || 0, 
          result.filesUploaded || 0,
          result.totalBytes || 0
        );
      }

      // Notify completion
      callback({
        totalFiles: 1,
        uploadedFiles: result.success ? 1 : 0,
        currentFile: result.success ? 'Upload completed successfully!' : 'Upload failed',
        bytesUploaded: result.totalBytes || 0,
        totalBytes: result.totalBytes || 0,
        speed: '0 MB/s',
        eta: result.success ? 'Complete' : 'Failed',
        isUploading: false
      });
    });
  }

  /**
   * Clean up event listeners and polling
   */
  public cleanup() {
    this.stopProgressPolling();
    ipcRenderer.removeAllListeners('upload-progress');
    ipcRenderer.removeAllListeners('upload-complete');
    
    // Clean up progress file
    try {
      if (fs.existsSync(this.progressFilePath)) {
        fs.unlinkSync(this.progressFilePath);
      }
    } catch (error) {
      console.error('Error cleaning up progress file:', error);
    }
  }
}

export default UploadService;