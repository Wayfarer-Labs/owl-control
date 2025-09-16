import React, { useState, useEffect } from "react";
// import { Logo } from '@/components/logo';
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
// Theme toggle removed - always dark theme
import { AuthService } from "@/services/auth-service";
import { PythonBridge, AppPreferences } from "@/services/python-bridge";
import { Check } from "lucide-react";
import { UploadPanel } from "@/components/upload-panel";

interface SettingsPageProps {
  onClose: () => void;
}

export function SettingsPage({ onClose }: SettingsPageProps) {
  const [statusMessage, setStatusMessage] = useState("");
  const [userInfo, setUserInfo] = useState<any>(null);
  const [startRecordingKey, setStartRecordingKey] = useState("f4");
  const [stopRecordingKey, setStopRecordingKey] = useState("f5");
  const [apiToken, setApiToken] = useState("");

  // Define the button styles directly in the component for reliability
  const buttonStyle = {
    backgroundColor: "hsl(186, 90%, 61%)",
    color: "#0c0c0f",
    border: "none",
    borderRadius: "0.5rem",
    padding: "0.5rem 1rem",
    cursor: "pointer",
    fontWeight: "medium",
    fontSize: "0.875rem",
    transition: "all 0.3s ease",
  };

  const authService = AuthService.getInstance();
  const pythonBridge = new PythonBridge();

  // Load preferences on component mount
  useEffect(() => {
    // Direct loading of credentials from Electron
    const isSettingsDirectNavigation =
      window.location.search.includes("page=settings");

    if (isSettingsDirectNavigation) {
      // When opened directly from system tray, load credentials directly from Electron
      try {
        const { ipcRenderer } = window.require("electron");
        ipcRenderer.invoke("load-credentials").then((result: any) => {
          if (result.success && result.data) {
            // Update auth service with the credentials
            const authService = AuthService.getInstance();
            if (result.data.apiKey) {
              authService.validateApiKey(result.data.apiKey);
            }
            if (result.data.hasConsented === "true") {
              authService.setConsent(true);
            }

            // Now load user info
            loadUserInfo();
          }
        });
      } catch (error) {
        console.error("Error loading credentials from Electron:", error);
      }
    }

    // Load preferences
    const prefs = pythonBridge.loadPreferences();
    if (prefs.startRecordingKey) setStartRecordingKey(prefs.startRecordingKey);
    if (prefs.stopRecordingKey) setStopRecordingKey(prefs.stopRecordingKey);
    if (prefs.apiToken) setApiToken(prefs.apiToken);

    // Always load user info after preferences
    loadUserInfo();
  }, []);

  const loadUserInfo = async () => {
    try {
      // Check if we're in direct settings mode
      if (
        (window as any).SKIP_AUTH === true ||
        (window as any).DIRECT_SETTINGS === true
      ) {
        // Try to load credentials directly from Electron
        try {
          const { ipcRenderer } = window.require("electron");
          const result = await ipcRenderer.invoke("load-credentials");
          if (result.success && result.data) {
            // Set credentials in auth service
            if (result.data.apiKey) {
              await authService.validateApiKey(result.data.apiKey);
            }
            if (result.data.hasConsented === "true") {
              await authService.setConsent(true);
            }
          }
        } catch (error) {
          console.error("Error loading credentials from Electron:", error);
        }
      }

      // Now get user info
      const info = await authService.getUserInfo();
      setUserInfo(info);
    } catch (error) {
      console.error("Error loading user info:", error);
    }
  };

  const savePreferences = () => {
    pythonBridge.savePreferences({
      startRecordingKey,
      stopRecordingKey,
      apiToken,
    });

    // After saving preferences, automatically start the Python bridges
    pythonBridge.startRecordingBridge();
    pythonBridge.startUploadBridge();
  };

  const handleLogout = async () => {
    await authService.logout();

    // Check if this is a direct settings window
    const isSettingsDirectNavigation =
      window.location.search.includes("page=settings");

    if (isSettingsDirectNavigation) {
      // Close the window via IPC if it's a direct settings window
      try {
        const { ipcRenderer } = window.require("electron");
        await ipcRenderer.invoke("close-settings");
      } catch (error) {
        console.error("Error closing settings window:", error);
      }
    } else {
      // Otherwise reload the page to show login
      window.location.reload();
    }
  };

  const handleSaveAndExit = () => {
    // Save preferences
    savePreferences();

    // Close window right after saving
    const isSettingsDirectNavigation =
      window.location.search.includes("page=settings");
    if (isSettingsDirectNavigation) {
      try {
        const { ipcRenderer } = window.require("electron");
        ipcRenderer.invoke("close-settings");
      } catch (error) {
        console.error("Error closing settings window:", error);
      }
    } else {
      onClose();
    }
  };

  return (
    <div className="fixed inset-0 bg-[#0c0c0f] z-50 flex flex-col select-none overflow-hidden">
      <div className="flex flex-col p-6 h-full overflow-hidden">
        {/* Header */}
        <div className="mb-6">
          <h1 className="text-2xl font-bold text-white select-none">
            Settings
          </h1>
          <p className="text-gray-400 select-none">
            Configure your recording preferences
          </p>
        </div>

        {/* Main Content */}
        <div className="flex-1 space-y-6 overflow-y-auto overflow-x-hidden pr-2">
          {/* Account Section */}
          {userInfo && (
            <div className="bg-[#13151a] rounded-lg border border-[#2a2d35] p-4">
              <h3 className="mb-2 text-sm font-medium text-white select-none">
                Account
              </h3>
              <div className="flex items-center justify-between">
                <p className="text-[#42e2f5] select-none">
                  {userInfo.email
                    ? `${userInfo.email}`
                    : "API Key authenticated"}
                </p>
                <button
                  className="bg-black text-white px-4 py-2 rounded-md font-medium select-none"
                  onClick={handleLogout}
                >
                  Logout
                </button>
              </div>
            </div>
          )}

          {/* API Token Section */}
          <div className="bg-[#13151a] rounded-lg border border-[#2a2d35] p-4">
            <h3 className="mb-4 text-sm font-medium text-white select-none">
              OWL API Token
            </h3>
            <div className="space-y-4">
              <div className="grid grid-cols-1 gap-4 items-center">
                <div>
                  <Input
                    id="apiToken"
                    value={apiToken}
                    onChange={(e) => setApiToken(e.target.value)}
                    className="bg-[#0c0c0f] border-[#2a2d35] text-white"
                    placeholder="Enter your OWL API token"
                  />
                </div>
              </div>
            </div>
          </div>

          {/* Keyboard Shortcuts */}
          <div className="bg-[#13151a] rounded-lg border border-[#2a2d35] p-4">
            <h3 className="mb-4 text-sm font-medium text-white select-none">
              Keyboard Shortcuts
            </h3>
            <div className="space-y-4">
              <div className="space-y-2">
                <Label
                  htmlFor="startRecordingKey"
                  className="text-sm text-white select-none"
                >
                  Start Recording
                </Label>
                <Input
                  id="startRecordingKey"
                  value={startRecordingKey}
                  onChange={(e) => setStartRecordingKey(e.target.value)}
                  placeholder="e.g., f4"
                  className="bg-[#0c0c0f] border-[#2a2d35] text-white"
                  onKeyDown={(e) => {
                    e.preventDefault();
                    const keys = [];
                    if (e.metaKey || e.ctrlKey) keys.push("CommandOrControl");
                    if (e.shiftKey) keys.push("Shift");
                    if (e.altKey) keys.push("Alt");
                    if (
                      e.key &&
                      e.key !== "Control" &&
                      e.key !== "Shift" &&
                      e.key !== "Alt" &&
                      e.key !== "Meta"
                    ) {
                      keys.push(e.key.toUpperCase());
                    }
                    setStartRecordingKey(keys.join("+"));
                  }}
                />
              </div>
              <div className="space-y-2">
                <Label
                  htmlFor="stopRecordingKey"
                  className="text-sm text-white select-none"
                >
                  Stop Recording
                </Label>
                <Input
                  id="stopRecordingKey"
                  value={stopRecordingKey}
                  onChange={(e) => setStopRecordingKey(e.target.value)}
                  placeholder="e.g., f5"
                  className="bg-[#0c0c0f] border-[#2a2d35] text-white"
                  onKeyDown={(e) => {
                    e.preventDefault();
                    const keys = [];
                    if (e.metaKey || e.ctrlKey) keys.push("CommandOrControl");
                    if (e.shiftKey) keys.push("Shift");
                    if (e.altKey) keys.push("Alt");
                    if (
                      e.key &&
                      e.key !== "Control" &&
                      e.key !== "Shift" &&
                      e.key !== "Alt" &&
                      e.key !== "Meta"
                    ) {
                      keys.push(e.key.toUpperCase());
                    }
                    setStopRecordingKey(keys.join("+"));
                  }}
                />
                <p className="text-xs text-gray-400">
                  Enter the key for starting/stopping recording (defaults:
                  f4/f5). Note: Only simple keys like F1-F12, or letters are
                  supported by the Python hotkey system.
                </p>
              </div>
            </div>
          </div>

          {/* Upload Manager */}
          <UploadPanel />
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between mt-6 pt-4 border-t border-[#2a2d35]">
          <div className="text-gray-500 text-sm select-none">
            Wayfarer Labs © {new Date().getFullYear()}
          </div>

          <button
            className="bg-[#42e2f5] text-black px-6 py-2 rounded-md font-medium select-none"
            onClick={handleSaveAndExit}
          >
            Save Settings
          </button>
        </div>
      </div>
    </div>
  );
}
