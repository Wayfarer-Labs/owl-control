import React, { useState, useEffect } from "react";
import { Upload, Clock, FileText, Wifi } from "lucide-react";
import UploadService, {
  UploadProgress,
  UploadStats,
} from "../services/upload-service";
import { ElectronService } from "../services/electron-service";

interface UploadPanelProps {
  isAuthenticated: boolean;
  className?: string;
}

export const UploadPanel: React.FC<UploadPanelProps> = ({
  className = "",
  isAuthenticated,
}) => {
  const [uploadService] = useState(() => UploadService.getInstance());
  const [isUploading, setIsUploading] = useState(false);
  const [progress, setProgress] = useState<UploadProgress>({
    totalFiles: 0,
    uploadedFiles: 0,
    currentFile: "",
    bytesUploaded: 0,
    totalBytes: 0,
    speed: "0 MB/s",
    eta: "--",
    isUploading: false,
  });
  const [stats, setStats] = useState<UploadStats>({
    totalDurationUploaded: 0,
    totalFilesUploaded: 0,
    totalVolumeUploaded: 0,
    lastUploadDate: "Never",
  });
  const [error, setError] = useState<string>("");

  useEffect(() => {
    // Load initial stats
    setStats(uploadService.getUploadStats());
    setIsUploading(uploadService.isUploading());

    // Cleanup on unmount
    return () => {
      uploadService.cleanup();
    };
  }, [uploadService]);

  const formatDuration = (seconds: number): string => {
    if (seconds === 0) return "0 min";

    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);

    if (hours > 0 && minutes > 0) {
      return `${hours}h ${minutes}m`;
    } else if (hours > 0) {
      return `${hours}h`;
    } else {
      return `${minutes}m`;
    }
  };

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  };

  const formatVolume = (bytes: number): string => {
    if (bytes === 0) return "0 MB";
    const k = 1024;
    const mb = bytes / (k * k);
    const gb = mb / k;

    if (gb >= 1) {
      return `${gb.toFixed(1)} GB`;
    } else {
      return `${mb.toFixed(1)} MB`;
    }
  };

  const formatDate = (dateString: string): string => {
    if (dateString === "Never") return "Never";
    try {
      return (
        new Date(dateString).toLocaleDateString() +
        " at " +
        new Date(dateString).toLocaleTimeString()
      );
    } catch {
      return "Unknown";
    }
  };

  const handleStartUpload = async () => {
    setError("");

    try {
      const credentialsResult = await ElectronService.loadCredentials();

      if (!credentialsResult.success || !credentialsResult.data.apiKey) {
        setError("No API key found. Please configure your API key first.");
        return;
      }

      const result = await uploadService.startUpload(
        credentialsResult.data.apiKey,
        (progressData) => {
          setProgress(progressData);
          setIsUploading(progressData.isUploading);

          // Update stats when upload completes
          if (!progressData.isUploading && progressData.uploadedFiles > 0) {
            setStats(uploadService.getUploadStats());
          }
        },
      );

      if (!result.success) {
        setError(result.message || "Failed to start upload");
      } else {
        setIsUploading(true);
      }
    } catch (error) {
      console.error("Upload error:", error);
      setError("Failed to start upload process");
    }
  };

  const progressPercentage =
    progress.totalFiles > 0
      ? Math.round((progress.uploadedFiles / progress.totalFiles) * 100)
      : 0;

  const bytesPercentage =
    progress.totalBytes > 0
      ? Math.round((progress.bytesUploaded / progress.totalBytes) * 100)
      : 0;

  return (
    <div
      className={`bg-[#13151a] border border-[#2a2d35] rounded-lg p-6 ${className}`}
    >
      {/* Header */}
      <div className="flex items-center gap-3 mb-6">
        <Upload className="h-4 w-4 text-[#42e2f5]" />
        <h2 className="text-sm font-medium text-white">Upload Manager</h2>
      </div>

      {/* Stats Cards */}
      <div className="grid grid-cols-4 gap-4 mb-6">
        <div className="bg-[#1f2028] border border-[#2a2d35] rounded-lg p-4">
          <div className="flex items-center gap-2 mb-2">
            <Clock className="h-4 w-4 text-[#42e2f5]" />
            <span className="text-sm text-gray-400">Total Uploaded</span>
          </div>
          <div className="text-lg font-bold text-white">
            {formatDuration(stats.totalDurationUploaded)}
          </div>
        </div>

        <div className="bg-[#1f2028] border border-[#2a2d35] rounded-lg p-4">
          <div className="flex items-center gap-2 mb-2">
            <FileText className="h-4 w-4 text-[#42e2f5]" />
            <span className="text-sm text-gray-400">Files Uploaded</span>
          </div>
          <div className="text-lg font-bold text-white">
            {stats.totalFilesUploaded}
          </div>
        </div>

        <div className="bg-[#1f2028] border border-[#2a2d35] rounded-lg p-4">
          <div className="flex items-center gap-2 mb-2">
            <Upload className="h-4 w-4 text-[#42e2f5]" />
            <span className="text-sm text-gray-400">Volume Uploaded</span>
          </div>
          <div className="text-lg font-bold text-white">
            {formatVolume(stats.totalVolumeUploaded)}
          </div>
        </div>

        <div className="bg-[#1f2028] border border-[#2a2d35] rounded-lg p-4">
          <div className="flex items-center gap-2 mb-2">
            <Wifi className="h-4 w-4 text-[#42e2f5]" />
            <span className="text-sm text-gray-400">Last Upload</span>
          </div>
          <div className="text-xs text-white">
            {formatDate(stats.lastUploadDate)}
          </div>
        </div>
      </div>

      {/* Upload Controls */}
      <div className="space-y-4">
        {error && (
          <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-3">
            <p className="text-red-400 text-sm">{error}</p>
          </div>
        )}

        {/* Progress Bar */}
        {isUploading && (
          <div className="space-y-3">
            <div className="flex items-center justify-between text-sm">
              <span className="text-gray-400">
                File {progress.uploadedFiles} of {progress.totalFiles}
              </span>
              <span className="text-gray-400">
                {progress.speed} • ETA: {progress.eta}
              </span>
            </div>

            <div className="w-full bg-[#2a2d35] rounded-full h-2">
              <div
                className="bg-[#42e2f5] h-2 rounded-full transition-all duration-300"
                style={{ width: `${progressPercentage}%` }}
              />
            </div>

            {progress.currentFile && (
              <p className="text-xs text-gray-400 truncate">
                Uploading: {progress.currentFile}
              </p>
            )}

            <div className="flex items-center justify-between text-xs text-gray-400">
              <span>Progress: {progressPercentage}%</span>
              <span>
                {formatBytes(progress.bytesUploaded)} /{" "}
                {formatBytes(progress.totalBytes)}
              </span>
            </div>
          </div>
        )}

        {/* Control Button */}
        <button
          onClick={handleStartUpload}
          disabled={isUploading || !isAuthenticated}
          className={`w-full flex items-center justify-center gap-2 px-4 py-3 rounded-md font-medium transition-all duration-200 ${
            isUploading || !isAuthenticated
              ? "bg-gray-600 cursor-not-allowed text-gray-300"
              : "bg-[#42e2f5] hover:bg-[#35c5d7] text-black"
          }`}
        >
          {isUploading ? "Upload in Progress..." : "Start Upload"}
        </button>

        <p className="text-xs text-gray-500 text-center">
          Upload your recorded gameplay data to contribute to the research
          dataset
        </p>
      </div>
    </div>
  );
};
