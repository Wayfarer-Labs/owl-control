import React, { useState, useEffect } from "react";
import { Logo } from "@/components/logo";
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
import { ThemeToggle } from "@/components/theme-toggle";
import { AuthService } from "@/services/auth-service";
import { PythonBridge } from "@/services/python-bridge";
import { Settings, Video, Home, LogOut, Info } from "lucide-react";

export function MainPage() {
  const [activeTab, setActiveTab] = useState<"home" | "settings">("home");
  const [apiToken, setApiToken] = useState("");
  const [statusMessage, setStatusMessage] = useState("Ready");
  const [userInfo, setUserInfo] = useState<any>(null);

  const authService = AuthService.getInstance();
  const pythonBridge = new PythonBridge();

  // Load preferences on component mount
  useEffect(() => {
    const prefs = pythonBridge.loadPreferences();
    if (prefs.apiToken) setApiToken(prefs.apiToken);

    loadUserInfo();

    // Start the Python bridges
    pythonBridge.startRecordingBridge();
    pythonBridge.startUploadBridge();
  }, []);

  const loadUserInfo = async () => {
    const info = await authService.getUserInfo();
    setUserInfo(info);
  };

  const savePreferences = () => {
    pythonBridge.savePreferences({
      apiToken,
    });
    setStatusMessage("API Token saved");
    setTimeout(() => setStatusMessage("Ready"), 2000);

    // Restart the upload bridge with the new API token
    pythonBridge.startUploadBridge();
  };

  const handleLogout = async () => {
    await authService.logout();
    window.location.reload();
  };

  return (
    <div className="min-h-screen bg-background">
      {/* Header */}
      <header className="border-b border-border">
        <div className="container flex h-16 items-center justify-between px-4">
          <div className="flex items-center space-x-2">
            <Logo width={32} height={32} />
            <span className="text-lg font-bold">OWL Control</span>
          </div>

          <div className="flex items-center space-x-4">
            {userInfo && (
              <div className="text-sm text-muted-foreground">
                {userInfo.email ? `${userInfo.email}` : "API Key Access"}
              </div>
            )}
            <ThemeToggle />
          </div>
        </div>
      </header>

      {/* Main content */}
      <main className="container py-6 px-4">
        <div className="flex gap-6">
          {/* Sidebar */}
          <aside className="hidden w-[200px] flex-col md:flex">
            <nav className="grid gap-2 px-2">
              <Button
                variant={activeTab === "home" ? "default" : "ghost"}
                className="justify-start"
                onClick={() => setActiveTab("home")}
              >
                <Home className="mr-2 h-4 w-4" />
                Home
              </Button>
              <Button
                variant={activeTab === "settings" ? "default" : "ghost"}
                className="justify-start"
                onClick={() => setActiveTab("settings")}
              >
                <Settings className="mr-2 h-4 w-4" />
                Settings
              </Button>
              <Button
                variant="ghost"
                className="justify-start"
                onClick={handleLogout}
              >
                <LogOut className="mr-2 h-4 w-4" />
                Logout
              </Button>
            </nav>
          </aside>

          {/* Content area */}
          <div className="flex-1">
            {activeTab === "home" ? (
              <div className="grid gap-6">
                <Card className="col-span-2">
                  <CardHeader>
                    <CardTitle>Control Panel</CardTitle>
                    <CardDescription>
                      Manage your recording session
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-6">
                    <div className="rounded-lg border p-4">
                      <div className="flex items-center justify-between">
                        <div className="flex items-center space-x-4">
                          <div className="rounded-full bg-primary/10 p-2">
                            <Info className="h-5 w-5 text-primary" />
                          </div>
                          <div>
                            <p className="text-sm font-medium">
                              Recording Status
                            </p>
                            <p className="text-sm text-muted-foreground">
                              Recording is handled by hotkeys (default: F4 to
                              start, F5 to stop)
                            </p>
                          </div>
                        </div>
                      </div>
                    </div>

                    <div className="rounded-lg border p-4">
                      <div className="mb-4">
                        <h3 className="text-sm font-medium">API Settings</h3>
                      </div>
                      <div className="grid gap-4">
                        <div className="grid grid-cols-3 items-center gap-4">
                          <Label htmlFor="apiToken" className="text-sm">
                            OWL API Token
                          </Label>
                          <Input
                            id="apiToken"
                            value={apiToken}
                            onChange={(e) => setApiToken(e.target.value)}
                            className="col-span-2 h-8"
                            placeholder="Enter your OWL API token"
                          />
                        </div>
                        <Button
                          onClick={savePreferences}
                          className="justify-self-end"
                        >
                          Save API Token
                        </Button>
                      </div>
                    </div>
                  </CardContent>
                </Card>
              </div>
            ) : (
              <div className="grid gap-6">
                <Card className="col-span-2">
                  <CardHeader>
                    <CardTitle>Settings</CardTitle>
                    <CardDescription>
                      Configure your recording preferences
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-6">
                    <div className="rounded-lg border p-4">
                      <h3 className="mb-4 text-sm font-medium">
                        Hotkey Information
                      </h3>
                      <div className="mb-4">
                        <p className="text-sm text-muted-foreground">
                          Recording is handled by the following default hotkeys:
                        </p>
                        <ul className="list-disc pl-5 mt-2 text-sm text-muted-foreground">
                          <li>Start Recording: F4</li>
                          <li>Stop Recording: F5</li>
                        </ul>
                        <p className="text-sm text-muted-foreground mt-2">
                          You can change these hotkeys in the Settings window
                          accessible from the system tray.
                        </p>
                      </div>
                      <h3 className="mb-4 text-sm font-medium">API Token</h3>
                      <div className="grid gap-4">
                        <div className="grid grid-cols-4 items-center gap-4">
                          <Label htmlFor="apiTokenFull" className="text-sm">
                            OWL API Token
                          </Label>
                          <Input
                            id="apiTokenFull"
                            value={apiToken}
                            onChange={(e) => setApiToken(e.target.value)}
                            className="col-span-3 h-8"
                            placeholder="Enter your OWL API token"
                          />
                        </div>
                      </div>
                    </div>

                    <div className="flex justify-end">
                      <Button onClick={savePreferences}>Save API Token</Button>
                    </div>
                  </CardContent>
                </Card>
              </div>
            )}
          </div>
        </div>
      </main>

      {/* Footer */}
      <footer className="border-t border-border py-4">
        <div className="container flex items-center justify-between px-4">
          <p className="text-sm text-muted-foreground">
            Wayfarer Labs &copy; {new Date().getFullYear()}
          </p>
          <p className="text-sm text-muted-foreground">Version 1.0.0</p>
        </div>
      </footer>
    </div>
  );
}
