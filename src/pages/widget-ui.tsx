import React, { useState, useEffect } from "react";
import { Settings, Info } from "lucide-react";
import { Button } from "@/components/ui/button";
import { PythonBridge } from "@/services/python-bridge";

interface WidgetUIProps {
  onOpenSettings: () => void;
}

export function WidgetUI({ onOpenSettings }: WidgetUIProps) {
  const [statusMessage, setStatusMessage] = useState("Ready");

  const pythonBridge = new PythonBridge();

  // Load preferences on component mount
  useEffect(() => {
    // Start the Python bridges
    pythonBridge.startRecordingBridge();
    pythonBridge.startUploadBridge();
  }, []);

  return (
    <div className="fixed top-4 right-4 p-3 bg-background/80 backdrop-blur-md border border-border rounded-lg shadow-lg transition-all duration-300 hover:bg-background">
      <div className="flex items-center space-x-3">
        <div className="flex flex-col">
          <div className="flex items-center space-x-2">
            <div className="w-3 h-3 rounded-full bg-green-500"></div>
            <span className="text-sm font-medium">
              Recording: F4 to Start, F5 to Stop
            </span>
          </div>
        </div>

        <div className="flex items-center space-x-2">
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 rounded-full"
            onClick={onOpenSettings}
          >
            <Settings className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}
