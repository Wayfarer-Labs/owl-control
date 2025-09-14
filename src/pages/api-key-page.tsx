import React, { useState } from "react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { AuthService } from "@/services/auth-service";

interface ApiKeyPageProps {
  onApiKeySuccess: () => void;
  onShowConsent: (apiKey: string) => void;
}

export function ApiKeyPage({
  onApiKeySuccess,
  onShowConsent,
}: ApiKeyPageProps) {
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState("");
  const [isLoading, setIsLoading] = useState(false);

  const authService = AuthService.getInstance();

  const handleApiKeyValidation = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setIsLoading(true);

    try {
      const result = await authService.validateApiKey(apiKey);

      if (result.success) {
        onShowConsent(apiKey);
      } else {
        setError(result.message || "Invalid API key");
      }
    } catch (err) {
      setError("Invalid API key");
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="h-full bg-[#0c0c0f]">
      {/* Draggable header area */}
      <div
        className="h-8"
        style={{ WebkitAppRegion: "drag", "-webkit-app-region": "drag" } as any}
      ></div>

      <div className="flex items-center justify-center h-[calc(100%-2rem)]">
        <div className="w-[380px] space-y-8">
          <div className="text-center">
            <h1 className="text-3xl font-bold text-white mb-2">Welcome</h1>
            <p className="text-[#828691]">Enter your API key to get started</p>
          </div>

          <form onSubmit={handleApiKeyValidation} className="space-y-5">
            <div>
              <Input
                id="apiKey"
                placeholder="sk_..."
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                required
                className="h-12 bg-[#1a1d23] border-[#2a2d35] text-white placeholder:text-[#4a4e58] focus:border-[#42e2f5] focus:ring-1 focus:ring-[#42e2f5]"
              />
              {error && <p className="text-[#ff5757] text-sm mt-2">{error}</p>}
            </div>

            <Button
              className="w-full h-12 bg-[#42e2f5] text-[#0c0c0f] font-medium hover:bg-[#42e2f5]/90 transition-colors"
              type="submit"
              disabled={isLoading}
            >
              {isLoading ? "Validating..." : "Continue"}
            </Button>
          </form>

          <p className="text-[#4a4e58] text-sm text-center">
            Wayfarer Labs © 2025
          </p>
        </div>
      </div>
    </div>
  );
}
