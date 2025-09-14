import { ElectronService } from "./electron-service";

export class AuthService {
  private static instance: AuthService;
  private apiKey: string | null = null;
  private hasConsented: boolean = false;

  private constructor() {
    // Try to load stored API key
    this.loadApiKey();
  }

  /**
   * Get singleton instance
   */
  public static getInstance(): AuthService {
    if (!AuthService.instance) {
      AuthService.instance = new AuthService();
    }
    return AuthService.instance;
  }

  /**
   * Load API key from storage
   */
  private async loadApiKey(): Promise<void> {
    try {
      const result = await ElectronService.loadCredentials();
      if (result.success && result.data.apiKey) {
        this.apiKey = result.data.apiKey;
        // Convert stored consent value to a strict boolean
        const consentVal = result.data.hasConsented;
        this.hasConsented = consentVal === true || consentVal === "true";
      } else {
        // Try to load from localStorage as fallback
        const storedKey = localStorage.getItem("apiKey");
        const storedConsent = localStorage.getItem("hasConsented");

        if (storedKey) {
          this.apiKey = storedKey;
          this.hasConsented = storedConsent === "true";

          // Save to secure storage
          await ElectronService.saveCredentials("apiKey", storedKey);
          if (this.hasConsented) {
            await ElectronService.saveCredentials("hasConsented", "true");
          }
        }
      }
    } catch (error) {
      console.error("Error loading API key:", error);
    }
  }

  /**
   * Check if user is authenticated and has consented
   */
  public isAuthenticated(): boolean {
    // Check if we're in direct settings mode (from Electron)
    if (
      (window as any).SKIP_AUTH === true ||
      (window as any).DIRECT_SETTINGS === true
    ) {
      return true;
    }
    return !!this.apiKey && this.hasConsented;
  }

  /**
   * Check if user has just an API key but hasn't consented
   */
  public hasApiKey(): boolean {
    return !!this.apiKey;
  }

  /**
   * Validate API key
   */
  public async validateApiKey(
    apiKey: string,
  ): Promise<{ success: boolean; message?: string }> {
    try {
      if (!apiKey || apiKey.trim() === "") {
        return { success: false, message: "API key cannot be empty" };
      }

      // Simple validation - check if it starts with 'sk_'
      if (!apiKey.startsWith("sk_")) {
        return { success: false, message: "Invalid API key format" };
      }

      // For now, accept any properly formatted key
      // In production, we would validate the key with the server

      // Store the API key
      this.apiKey = apiKey;

      // Save to secure storage
      await ElectronService.saveCredentials("apiKey", apiKey);

      // Also save to localStorage as fallback
      try {
        localStorage.setItem("apiKey", apiKey);
      } catch (error) {
        console.error("Error saving API key to localStorage:", error);
      }

      return { success: true };
    } catch (error) {
      console.error("API key validation error:", error);
      return { success: false, message: "API key validation failed" };
    }
  }

  /**
   * Set consent status
   */
  public async setConsent(hasConsented: boolean): Promise<void> {
    this.hasConsented = hasConsented;

    // Save to secure storage
    await ElectronService.saveCredentials(
      "hasConsented",
      hasConsented ? "true" : "false",
    );

    // Also save to localStorage as fallback
    try {
      localStorage.setItem("hasConsented", hasConsented ? "true" : "false");
    } catch (error) {
      console.error("Error saving consent status to localStorage:", error);
    }
  }

  /**
   * Get upload URL for game control data
   */
  public async getUploadUrl(options: {
    filename: string;
    content_type: string;
    file_size_mb: number;
    expiration: number;
    video_filename: string;
    control_filename: string;
    tags?: string[];
  }): Promise<{ success: boolean; uploadUrl?: string; message?: string }> {
    try {
      if (!this.isAuthenticated()) {
        return {
          success: false,
          message: "Not authenticated or no consent given",
        };
      }

      // In a real implementation, this would make an HTTP request to the server
      // For now, we'll mock the response

      const mockResponse = {
        success: true,
        uploadUrl: `https://upload.openworldlabs.com/presigned/${options.filename}?token=${Math.random().toString(36).substring(2, 15)}`,
      };

      return mockResponse;
    } catch (error) {
      console.error("Error getting upload URL:", error);
      return { success: false, message: "Failed to get upload URL" };
    }
  }

  /**
   * Get user information
   */
  public async getUserInfo(): Promise<any> {
    if (!this.apiKey) {
      return { authenticated: false };
    }

    // Return user info
    return {
      authenticated: this.isAuthenticated(),
      hasApiKey: this.hasApiKey(),
      hasConsented: this.hasConsented,
      method: "apiKey",
      apiKey: this.apiKey && this.apiKey.substring(0, 10) + "...",
    };
  }

  /**
   * Upload game control data
   */
  public async uploadGameControlData(options: {
    filename: string;
    content_type: string;
    file_size_mb: number;
    video_filename: string;
    control_filename: string;
    tags?: string[];
  }): Promise<{ success: boolean; message?: string }> {
    try {
      if (!this.isAuthenticated()) {
        return {
          success: false,
          message: "Not authenticated or no consent given",
        };
      }

      // 1. Get upload URL
      const urlResult = await this.getUploadUrl({
        ...options,
        expiration: 3600, // 1 hour expiration
      });

      if (!urlResult.success || !urlResult.uploadUrl) {
        return {
          success: false,
          message: urlResult.message || "Failed to get upload URL",
        };
      }

      // 2. In a real implementation, we would use the URL to upload the file
      // For now, just return success

      return { success: true };
    } catch (error) {
      console.error("Error uploading game control data:", error);
      return { success: false, message: "Failed to upload game control data" };
    }
  }

  /**
   * Logout user
   */
  public async logout(): Promise<void> {
    this.apiKey = null;
    this.hasConsented = false;

    // Remove from secure storage
    await ElectronService.saveCredentials("apiKey", "");
    await ElectronService.saveCredentials("hasConsented", "false");

    // Also remove from localStorage
    try {
      localStorage.removeItem("apiKey");
      localStorage.removeItem("hasConsented");
    } catch (error) {
      console.error("Error removing credentials from localStorage:", error);
    }
  }
}
