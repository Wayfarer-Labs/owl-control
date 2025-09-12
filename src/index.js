import React, { useState, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider } from "./components/theme-provider";
import { ApiKeyPage } from "./pages/api-key-page";
import { ConsentPage } from "./pages/consent-page";
import { SettingsPage } from "./pages/settings-page";
import { AuthService } from "./services/auth-service";

// Import CSS styles
import "./styles.css";
import "./settings-styles.css";

// Import assets to ensure they're included in the build
import "./assets/tray-icon.svg";

// Log app start for debugging
console.log("OWL Control app initializing...");

const App = () => {
  const [authenticated, setAuthenticated] = useState(false);
  const [showConsent, setShowConsent] = useState(false);
  const [pendingApiKey, setPendingApiKey] = useState("");
  const [showSettings, setShowSettings] = useState(false);

  const authService = AuthService.getInstance();

  // Check if authenticated on load and handle direct navigation
  useEffect(() => {
    // Check if we're in direct settings mode from Electron
    if (window.SKIP_AUTH === true || window.DIRECT_SETTINGS === true) {
      console.log("Direct settings mode detected - skipping auth check");
      // Force authenticated state and show settings
      setAuthenticated(true);
      setShowSettings(true);
      return;
    }

    // Regular authentication check
    const isAuth = authService.isAuthenticated();
    setAuthenticated(isAuth);

    // Check if we should show settings directly (from URL parameter)
    const urlParams = new URLSearchParams(window.location.search);
    const page = urlParams.get("page");

    if (page === "settings" && isAuth) {
      // If this is a settings page request and user is authenticated, show settings
      setShowSettings(true);
    }
  }, []);

  const handleApiKeySuccess = () => {
    setAuthenticated(true);
  };

  const handleShowConsent = async (apiKey) => {
    setPendingApiKey(apiKey);
    setShowConsent(true);

    // Resize window for consent page
    try {
      const { ipcRenderer } = window.require("electron");
      await ipcRenderer.invoke("resize-for-consent");
    } catch (error) {
      console.error("Error resizing window for consent:", error);
    }
  };

  const handleConsent = () => {
    setAuthenticated(true);
    setShowConsent(false);
  };

  const handleCancelConsent = async () => {
    setPendingApiKey("");
    setShowConsent(false);

    // Resize window back to API key size
    try {
      const { ipcRenderer } = window.require("electron");
      await ipcRenderer.invoke("resize-for-api-key");
    } catch (error) {
      console.error("Error resizing window for API key:", error);
    }
  };

  const handleCloseSettings = () => {
    setShowSettings(false);
  };

  // Check for settings navigation via URL or global flags (from Electron)
  const isSettingsDirectNavigation =
    window.location.search.includes("page=settings") ||
    window.location.search.includes("direct=true") ||
    window.location.hash === "#settings" ||
    window.SKIP_AUTH === true ||
    window.DIRECT_SETTINGS === true;

  // Create an early detection/debug function
  useEffect(() => {
    console.log("OWL Control init with URL:", window.location.href);
    console.log("Direct settings detected:", isSettingsDirectNavigation);
    console.log("Auth state:", authenticated);

    // Force dark theme on all loads
    document.documentElement.classList.add("dark");
    document.documentElement.style.colorScheme = "dark";
    document.body.style.backgroundColor = "#0c0c0f";
    document.body.style.color = "#f8f9fa";

    // IMPORTANT: These direct css edits will apply immediately
    document.body.style.backgroundColor = "#0c0c0f";
    document.body.style.color = "#f8f9fa";

    // Create a style element to ensure our theme is applied
    const styleEl = document.createElement("style");
    styleEl.textContent = `
      :root {
        --background: 240 12% 5%;
        --foreground: 210 17% 98%;
        --card: 222 14% 9%;
        --card-foreground: 210 17% 98%;
        --primary: 186 90% 61%;
        --primary-foreground: 240 12% 5%;
        --secondary: 157 74% 67%;
        --secondary-foreground: 240 12% 5%;
      }
      
      /* Force ALL button components to use correct colors */
      button, 
      [role="button"], 
      [type="button"],
      .Button,
      [class*="Button"],
      [data-radix-popper-content-wrapper] button {
        background-color: hsl(186, 90%, 61%) !important;
        color: #0c0c0f !important;
        border: none !important;
        transition: none !important;
        transform: none !important;
        opacity: 1 !important;
        visibility: visible !important;
        display: inline-block !important;
        animation: none !important;
        animation-fill-mode: none !important;
      }
      
      /* Extra specific button selectors */
      html body #root button,
      html body #root [role="button"],
      html [class*="settings"] button,
      html [class*="card"] button,
      #root button,
      .rounded-lg button,
      button.button-shine,
      button.glow {
        background-color: hsl(186, 90%, 61%) !important;
        color: #0c0c0f !important;
        opacity: 1 !important;
        visibility: visible !important;
        display: inline-block !important;
        transform: none !important;
        transition: none !important;
        animation: none !important;
      }
      
      /* Fix any specificity issues with Tailwind */
      .bg-primary, 
      .bg-\\[hsl\\(186, 
      [class*="bg-primary"], 
      [class*="bg-\\[hsl\\(186"], 
      .Button, 
      button.Button {
        background-color: hsl(186, 90%, 61%) !important;
        color: #0c0c0f !important;
        opacity: 1 !important;
        visibility: visible !important;
        display: inline-block !important;
      }
      
      /* Hover effects */
      button:hover, 
      [role="button"]:hover, 
      [type="button"]:hover,
      .Button:hover {
        background-color: hsl(186, 90%, 55%) !important;
      }
      
      /* Special hover effects for the glowing buttons */
      button.glow:hover, 
      button.button-shine:hover {
        background-color: hsl(186, 90%, 55%) !important;
        box-shadow: 0 0 15px rgba(66, 226, 245, 0.5) !important;
        transform: translateY(-1px) !important;
      }
      
      /* Active state */
      button:active, 
      [role="button"]:active, 
      [type="button"]:active {
        transform: translateY(1px) !important;
      }
      
      /* Dark background for cards */
      [class*="rounded-lg"], [class*="border"], [class*="shadow"], [class*="p-"], [class*="bg-popover"],
      [class*="bg-card"], [class*="card"] {
        background-color: #13151a !important;
        border-color: #2a2d35 !important;
      }
      
      /* Force dark background for root elements */
      html, body, #root {
        background-color: #0c0c0f !important;
        color: #f8f9fa !important;
      }
      
      /* Aurora background effect */
      body::before {
        content: '';
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        z-index: -1;
        background: linear-gradient(
          90deg, 
          transparent 0%, 
          rgba(66, 226, 245, 0.015) 15%, 
          rgba(77, 206, 129, 0.02) 30%, 
          transparent 50%, 
          rgba(66, 226, 245, 0.015) 70%, 
          rgba(77, 206, 129, 0.02) 85%, 
          transparent 100%
        );
        opacity: 0.7;
        animation: aurora 20s linear infinite;
      }
      
      @keyframes aurora {
        0% { background-position: 0% 0%; }
        100% { background-position: 100% 0%; }
      }
    `;
    document.head.appendChild(styleEl);
  }, []);

  // Handle direct navigation to settings
  if (isSettingsDirectNavigation) {
    console.log(
      "Direct navigation to settings detected - rendering settings view",
    );

    // Force authentication immediately when coming from system tray
    if (!authenticated) {
      console.log("Forcing authentication for settings view");
      // Direct auth without going through normal flow
      authService.setConsent(true);
    }

    // Render only settings - no auth layer
    return React.createElement(
      ThemeProvider,
      { defaultTheme: "dark" },
      React.createElement(SettingsPage, { onClose: handleCloseSettings }),
    );
  }

  // Render the appropriate UI based on authentication state
  return React.createElement(
    ThemeProvider,
    { defaultTheme: "dark" },
    !authenticated
      ? showConsent
        ? React.createElement(ConsentPage, {
            apiKey: pendingApiKey,
            onConsent: handleConsent,
            onCancel: handleCancelConsent,
          })
        : React.createElement(ApiKeyPage, {
            onApiKeySuccess: handleApiKeySuccess,
            onShowConsent: handleShowConsent,
          })
      : React.createElement(SettingsPage, { onClose: handleCloseSettings }),
  );
};

const root = createRoot(document.getElementById("root"));
root.render(React.createElement(App));
