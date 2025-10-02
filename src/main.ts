import {
  app,
  BrowserWindow,
  ipcMain,
  dialog,
  Tray,
  Menu,
  nativeImage,
  shell,
  MenuItem,
  MenuItemConstructorOptions,
} from "electron";
import * as path from "path";
import * as fs from "fs";
import {
  ChildProcessWithoutNullStreams,
  spawn,
  SpawnOptionsWithoutStdio,
} from "child_process";
import { join } from "path";

import "./log";
import {
  Credentials,
  DEFAULT_CREDENTIALS,
  DEFAULT_PREFERENCES,
  IpcResponse,
  Preferences,
} from "./settings";

// Log app startup
console.log(`=== OWL Control v${app.getVersion()} Debug Log Started ===`);

// Keep references
let mainWindow: BrowserWindow | null = null;
let settingsWindow: BrowserWindow | null = null;
let tray: Tray | null = null;

let recordingBridgeProcess: ChildProcessWithoutNullStreams | null = null;
let recordingRestartInterval: NodeJS.Timeout | null = null;
let isRecording = false;

// Secure store for credentials and preferences
const secureStore = {
  credentials: DEFAULT_CREDENTIALS,
  preferences: DEFAULT_PREFERENCES,
};

// Path to store config
const configPath = join(app.getPath("userData"), "config.json");

// Load config if it exists
function loadConfig() {
  try {
    if (fs.existsSync(configPath)) {
      const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
      if (config.credentials) {
        secureStore.credentials = {
          ...DEFAULT_CREDENTIALS,
          ...config.credentials,
        };
      }
      if (config.preferences) {
        secureStore.preferences = {
          ...DEFAULT_PREFERENCES,
          ...config.preferences,
        };
      }
    }
  } catch (error) {
    console.error("Error loading config:", error);
  }
}

// Save config
function saveConfig() {
  try {
    fs.writeFileSync(
      configPath,
      JSON.stringify({
        credentials: secureStore.credentials,
        preferences: secureStore.preferences,
      }),
    );
  } catch (error) {
    console.error("Error saving config:", error);
  }
}

// Check if authenticated
function isAuthenticated() {
  return (
    secureStore.credentials.apiKey &&
    (secureStore.credentials.hasConsented === "true" ||
      secureStore.credentials.hasConsented === true)
  );
}

// Create the main window
function createMainWindow() {
  if (mainWindow) {
    mainWindow.close();
  }

  mainWindow = new BrowserWindow({
    width: 440,
    height: 380,
    autoHideMenuBar: true,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false,
      preload: path.join(__dirname, "preload.js"),
    },
    frame: true,
    transparent: false,
    resizable: true,
    fullscreenable: false,
    minimizable: true,
    maximizable: true,
    backgroundColor: "#0c0c0f",
    center: true,
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "default",
    title: "OWL Control",
  });

  // Load index.html
  mainWindow.loadFile(path.join(__dirname, "index.html"));

  mainWindow.on("closed", () => {
    mainWindow = null;
  });
}

// Create settings window
function createSettingsWindow() {
  if (settingsWindow) {
    settingsWindow.focus();
    return;
  }

  // Create settings window with proper flags
  settingsWindow = new BrowserWindow({
    width: 800,
    height: 630,
    autoHideMenuBar: true,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false,
      preload: path.join(__dirname, "preload.js"),
    },
    parent: mainWindow || undefined,
    modal: false,
    show: false,
    backgroundColor: "#0c0c0f",
    resizable: true,
    fullscreenable: false,
    minimizable: true,
    maximizable: true,
    frame: true,
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "default",
    title: "OWL Control",
  });

  // Directly load settings page with query parameters - single load
  settingsWindow.loadURL(
    "file://" +
      path.join(__dirname, "index.html?page=settings&direct=true#settings"),
  );

  // Use the global switch-to-widget handler from setupIpcHandlers

  // Set up DOM ready handler to apply CSS immediately
  settingsWindow.webContents.on("dom-ready", () => {
    // Apply simple CSS to prevent white flash during load
    settingsWindow!.webContents.insertCSS(`
      html, body { background-color: #0c0c0f !important; }
      #root { background-color: #0c0c0f !important; }
    `);
  });

  settingsWindow.webContents.once("did-finish-load", () => {
    // Restore the original full CSS with detailed styling
    const css = `
      /* Force dark mode throughout the app */
      html, body, #root, [class*="bg-background"], .bg-background {
        background-color: #0c0c0f !important;
        color: #f8f9fa !important;
      }

      /* Fix black box issues - make text containers transparent */
      h1, h2, h3, h4, h5, h6, p, span, label, a,
      div.text-sm, div.text-muted-foreground, div.flex.items-center {
        background-color: transparent !important;
      }

      /* Tailwind card styles */
      .bg-card, [class*="bg-card"], div[class*="card"] {
        background-color: #13151a !important;
        border-color: #2a2d35 !important;
        color: #f8f9fa !important;
      }

      /* Handle all card variations */
      [class*="rounded-lg"], [class*="border"], [class*="shadow"], [class*="p-"], [class*="bg-popover"] {
        background-color: #13151a !important;
        border-color: #2a2d35 !important;
      }

      /* Button styling - cyan accent color */
      button, [role="button"], [type="button"] {
        background-color: hsl(186, 90%, 61%) !important;
        color: #0c0c0f !important;
        border: none !important;
      }

      /* Primary button styling */
      button[class*="primary"], [role="button"][class*="primary"], .btn-primary, .button-primary {
        background-color: hsl(186, 90%, 61%) !important;
        color: #0c0c0f !important;
        border: none !important;
      }

      /* Button variants */
      [class*="btn-secondary"], [class*="btn-outline"], [class*="ghost"] {
        background-color: transparent !important;
        border-color: #2a2d35 !important;
        color: #f8f9fa !important;
      }

      /* Text colors */
      p, h1, h2, h3, h4, h5, h6, span, label {
        color: #f8f9fa !important;
      }

      /* Tailwind specific text classes */
      [class*="text-"], [class*="text-muted"] {
        color: #f8f9fa !important;
      }

      /* Secondary text */
      [class*="text-muted"], [class*="text-secondary"] {
        color: #a0aec0 !important;
      }

      /* Fix any potential black boxes around titles and text content */
      [class*="card-header"], [class*="card-title"], [class*="card-description"],
      .text-sm, .text-muted-foreground, .col-span-2, .flex.items-center,
      p.text-sm, div.text-sm, .space-y-2, .space-y-4 {
        background-color: transparent !important;
      }

      /* Fix black boxes around specific elements */
      .col-span-2 *, div.flex.items-center *, .rounded-lg.border.p-4 * {
        background-color: transparent !important;
      }

      /* Fix any specific components */
      .fixed.inset-0.bg-background\\/80.backdrop-blur-sm.z-50 {
        background-color: rgba(12, 12, 15, 0.8) !important;
      }

      /* Ensure all UI components are dark */
      .bg-background, .bg-card, [class*="muted"], [class*="popover"] {
        background-color: #13151a !important;
      }
    `;

    // Inject CSS first
    settingsWindow!.webContents.insertCSS(css);

    // First set credentials to ensure auth works
    settingsWindow!.webContents
      .executeJavaScript(
        `
      document.documentElement.classList.add('dark');

      // Force dark theme using body class as well
      document.body.classList.add('dark-theme');

      // We're ready to show the window now
      true; // Return value for promise
    `,
      )
      .then(() => {
        // After the page has applied dark theme, we can safely show the window
        if (settingsWindow) {
          settingsWindow.show();
        }
      });
  });

  settingsWindow.on("closed", () => {
    settingsWindow = null;
  });
}

// Create tray icon
function createTray() {
  try {
    console.log('Creating tray icon - cyan with dark "OWL" text');

    // Use a simple 16x16 image created directly with Electron's nativeImage
    const size = { width: 16, height: 16 };
    const icon = nativeImage.createEmpty();

    // Define colors
    const cyan = { r: 66, g: 226, b: 245, a: 255 }; // #42E2F5 - Cyan background
    const dark = { r: 12, g: 12, b: 15, a: 255 }; // #0C0C0F - Dark text

    // Create buffer for the icon
    const trayIconBuffer = Buffer.alloc(size.width * size.height * 4);

    // First, fill the entire buffer with cyan (background)
    for (let i = 0; i < trayIconBuffer.length; i += 4) {
      trayIconBuffer[i] = cyan.b; // B
      trayIconBuffer[i + 1] = cyan.g; // G
      trayIconBuffer[i + 2] = cyan.r; // R
      trayIconBuffer[i + 3] = cyan.a; // A
    }

    // Define letters with points (1 = dark pixel, 0 = background)
    const textMap = [
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 0
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 1
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 2
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 3
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 4
      [0, 0, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0], // Row 5: O W L
      [0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0], // Row 6: O W L
      [0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0], // Row 7: O W L
      [0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0], // Row 8: O W L
      [0, 0, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 0, 0, 0], // Row 9: O W L
      [0, 0, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 0], // Row 10: O W L
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 11
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 12
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 13
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 14
      [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Row 15
    ];

    // Override the cyan background with text pixels
    for (let y = 0; y < size.height; y++) {
      for (let x = 0; x < size.width; x++) {
        if (textMap[y][x] === 1) {
          // Calculate position in buffer (4 bytes per pixel)
          const pos = (y * size.width + x) * 4;
          // Set the dark pixel for text
          trayIconBuffer[pos] = dark.b; // B
          trayIconBuffer[pos + 1] = dark.g; // G
          trayIconBuffer[pos + 2] = dark.r; // R
          trayIconBuffer[pos + 3] = dark.a; // A
        }
      }
    }

    // Create the icon from the buffer data
    icon.addRepresentation({
      width: size.width,
      height: size.height,
      buffer: trayIconBuffer,
      scaleFactor: 1.0,
    });

    console.log("Created cyan tray icon with OWL text");

    // Create tray with our icon
    tray = new Tray(icon);

    // On macOS, we can use a title to ensure visibility
    if (process.platform === "darwin") {
      tray.setTitle("VG");
    }

    updateTrayMenu();

    // Double-click on tray icon opens settings
    tray.on("double-click", () => {
      if (isAuthenticated()) {
        createSettingsWindow();
      } else {
        createMainWindow();
      }
    });
  } catch (error) {
    console.error("Error creating tray:", error);
  }
}

// Update the tray menu
function updateTrayMenu() {
  if (!tray) return;

  const menuTemplate: (MenuItemConstructorOptions | MenuItem)[] = [];

  // Add status item
  menuTemplate.push({
    label: isRecording ? "Recording..." : "Not Recording",
    enabled: false,
  });

  menuTemplate.push({ type: "separator" });

  // Remove recording controls from menu as Python bridge handles this independently
  if (isAuthenticated()) {
    menuTemplate.push({ type: "separator" });

    menuTemplate.push({
      label: "Settings",
      click: () => createSettingsWindow(),
    });
  } else {
    menuTemplate.push({
      label: "Setup",
      click: () => createMainWindow(),
    });
  }

  menuTemplate.push({ type: "separator" });

  menuTemplate.push({
    label: "Help",
    click: () => {
      shell.openExternal("https://wayfarerlabs.ai/contribute");
    },
  });

  menuTemplate.push({
    label: "Quit",
    click: () => {
      app.quit();
    },
  });

  // Update tray icon color/label based on recording state
  if (process.platform === "darwin") {
    tray.setTitle(isRecording ? "Recording" : "");
  }

  const contextMenu = Menu.buildFromTemplate(menuTemplate);
  tray.setContextMenu(contextMenu);
  tray.setToolTip(isRecording ? "OWL Control - Recording" : "OWL Control");
}

// Ensure Python dependencies are installed
async function ensurePythonDependencies() {
  return new Promise<boolean>((resolve) => {
    console.log("Installing Python dependencies...");
    console.log("STARTUP: Installing Python dependencies");

    const installProcess = spawnUv(["sync"], {
      cwd: rootDir(),
    });

    installProcess.stdout.on("data", (data: Buffer) => {
      const output = data.toString();
      console.log(`Dependency install stdout: ${output}`);
      console.log(`DEP_INSTALL: ${output.trim()}`);
    });

    installProcess.stderr.on("data", (data: Buffer) => {
      const output = data.toString();
      console.error(`Dependency install stderr: ${output}`);
      console.log(`DEP_INSTALL_ERR: ${output.trim()}`);
    });

    installProcess.on("close", (code: number) => {
      if (code === 0) {
        console.log("Python dependencies installed successfully");
        console.log(
          "STARTUP: Python dependencies installation completed successfully",
        );
        resolve(true);
      } else {
        console.error(`Dependency installation failed with code ${code}`);
        console.log(
          `STARTUP: Python dependencies installation failed with code ${code}`,
        );
        resolve(false);
      }
    });

    installProcess.on("error", (error: Error) => {
      console.error("Error installing Python dependencies:", error);
      console.log(
        `STARTUP: Error installing Python dependencies: ${error.message}`,
      );
      resolve(false);
    });
  });
}

function startRecordingBridge(reason: string) {
  try {
    // Stop existing process if running
    if (recordingBridgeProcess) {
      console.log("Stopping existing recording bridge");
      recordingBridgeProcess.kill();

      // Keep trying to restart until the previous process is dead
      if (recordingRestartInterval) {
        clearInterval(recordingRestartInterval);
        recordingRestartInterval = null;
        console.log("Stopped existing recording restart interval");
      }

      console.log("Starting new recording restart interval");
      recordingRestartInterval = setInterval(() => {
        if (recordingBridgeProcess) {
          console.log(
            "Can't start new recording bridge, old process is still running",
          );
          return;
        }

        console.log("Old process is dead, starting new recording bridge");
        startRecordingBridgeImmediately(reason);

        if (recordingRestartInterval) {
          clearInterval(recordingRestartInterval);
          recordingRestartInterval = null;
        }
      }, 250);
    } else {
      console.log("Starting recording bridge immediately");
      startRecordingBridgeImmediately(reason);
    }
  } catch (error) {
    console.error("Error starting recording bridge:", error);
  }
}

function startRecordingBridgeImmediately(reason: string) {
  console.log(`Starting recording bridge (reason: ${reason})`);
  console.log(`Executing: ${recorderCommand()}`);
  console.log(`Working directory: ${rootDir()}`);

  recordingBridgeProcess = spawn(
    recorderCommand(),
    [
      "--recording-location",
      "./data_dump/games/",
      "--start-key",
      secureStore.preferences.startRecordingKey,
      "--stop-key",
      secureStore.preferences.stopRecordingKey,
    ],
    {
      cwd: rootDir(),
    },
  );

  // Handle output
  recordingBridgeProcess.stdout.on("data", (data: Buffer) => {
    console.log(`Recording bridge stdout: ${data.toString()}`);
  });

  recordingBridgeProcess.stderr.on("data", (data: Buffer) => {
    console.error(`Recording bridge stderr: ${data.toString()}`);
  });

  recordingBridgeProcess.on("error", (error: Error) => {
    console.error(`Recording bridge process error: ${error.message}`);
    console.error(
      `This usually means the executable was not found: ${recorderCommand()}`,
    );
  });

  recordingBridgeProcess.on("close", (code: number | null) => {
    console.log(`Recording bridge process exited with code ${code}`);
    if (code !== 0 && code !== null) {
      console.error(`Recording bridge failed with exit code ${code}`);
    }
    recordingBridgeProcess = null;
  });
}

function recorderCommand() {
  if (process.env.NODE_ENV === "development") {
    return String.raw`target\x86_64-pc-windows-msvc\debug\owl-recorder`;
  } else {
    return "owl-recorder";
  }
}

function rootDir() {
  if (process.env.NODE_ENV === "development") {
    return ".";
  } else {
    // In packaged app, use the resources directory
    return process.resourcesPath;
  }
}

/**
 * Spawns Python code with a bundled / development uv.
 */
function spawnUv(args: string[], options?: SpawnOptionsWithoutStdio) {
  const isDevelopment = process.env.NODE_ENV === "development";
  // Use system uv in development, bundled uv in production
  const uvPath = isDevelopment
    ? "uv"
    : path.join(process.resourcesPath, "uv.exe");
  let env: Record<string, string> = {
    ...(options?.env || {}),
    // Do not attempt to update the dependencies
    UV_FROZEN: "1",
  };

  if (!isDevelopment) {
    // In production, we override all of uv's paths to ensure that it installs everything locally
    // to OWLC, which should stop it from interfering with the user's global state and/or
    // have a better chance of working in non-standard configurations
    const uvDir = path.join(process.resourcesPath, "uv");
    if (!fs.existsSync(uvDir)) {
      fs.mkdirSync(uvDir, { recursive: true });
    }

    env = {
      ...env,
      // Always copy deps, do not hardlink
      UV_LINK_MODE: "copy",
      // Do not let the user's configuration interfere with our uv
      UV_NO_CONFIG: "1",
      // Mark all dependencies as non-editable
      UV_NO_EDITABLE: "1",
      // Ensure we always use our managed Python
      UV_MANAGED_PYTHON: "1",
      // Update all directories
      UV_CACHE_DIR: path.join(uvDir, "cache"),
      UV_PYTHON_INSTALL_DIR: path.join(uvDir, "python_install"),
      UV_PYTHON_BIN_DIR: path.join(uvDir, "python_bin"),
      UV_TOOL_DIR: path.join(uvDir, "tool"),
      UV_TOOL_BIN_DIR: path.join(uvDir, "tool_bin"),
    };
  }

  return spawn(uvPath, args, {
    ...(options || {}),
    env,
  });
}

// App ready event
app.on("ready", async () => {
  // Load config
  loadConfig();

  // Set up IPC handlers
  setupIpcHandlers();

  // Create the tray
  createTray();

  // Install Python dependencies before starting any Python processes
  const depsInstalled = await ensurePythonDependencies();
  if (!depsInstalled) {
    console.warn(
      "Failed to install Python dependencies, Python features may not work correctly",
    );
    console.log(
      "STARTUP: Python dependencies installation failed, continuing anyway",
    );
  }

  startRecordingBridge("ready");

  // If not authenticated, show main window for setup
  if (!isAuthenticated()) {
    createMainWindow();
  }
});

// Prevent app from closing when all windows are closed (keep tray icon)
app.on("window-all-closed", () => {
  // Do nothing to keep the app running in the background
});

app.on("activate", () => {
  if (mainWindow === null) {
    if (!isAuthenticated()) {
      createMainWindow();
    }
  }
});

// Quit app completely when exiting
app.on("before-quit", () => {
  // Kill the recording bridge process
  if (recordingBridgeProcess) {
    recordingBridgeProcess.kill();
    recordingBridgeProcess = null;
  }
});

// Set up IPC handlers
function setupIpcHandlers() {
  ipcMain.on("log-to-file", (_event, level, message) => {
    console.log(`RENDERER-${level}: ${message}`);
  });

  // Open directory dialog
  ipcMain.handle("open-directory-dialog", async () => {
    if (!mainWindow && !settingsWindow) return "";

    const parentWindow = settingsWindow || mainWindow;
    const result = await dialog.showOpenDialog(parentWindow!, {
      properties: ["openDirectory"],
    });

    if (result.canceled || result.filePaths.length === 0) {
      return "";
    }

    return result.filePaths[0];
  });

  // Open save dialog
  ipcMain.handle("open-save-dialog", async () => {
    if (!mainWindow && !settingsWindow) return "";

    const parentWindow = settingsWindow || mainWindow;
    const result = await dialog.showSaveDialog(parentWindow!, {
      properties: ["createDirectory"],
    });

    if (result.canceled || !result.filePath) {
      return "";
    }

    return result.filePath;
  });

  // Save credentials
  ipcMain.handle(
    "save-credentials",
    async (
      _,
      credentials: Partial<Credentials>,
    ): Promise<IpcResponse<void>> => {
      try {
        secureStore.credentials = {
          ...secureStore.credentials,
          ...credentials,
        };
        saveConfig();

        updateTrayMenu();

        return { success: true, data: undefined };
      } catch (error) {
        console.error("Error saving credentials:", error);
        return { success: false, error: String(error) };
      }
    },
  );

  // Load credentials
  ipcMain.handle(
    "load-credentials",
    async (): Promise<IpcResponse<Credentials>> => {
      try {
        return {
          success: true,
          data: { ...DEFAULT_CREDENTIALS, ...secureStore.credentials },
        };
      } catch (error) {
        console.error("Error loading credentials:", error);
        return { success: false, error: String(error) };
      }
    },
  );

  // Save preferences
  ipcMain.handle(
    "save-preferences",
    async (_, preferences: any): Promise<IpcResponse<void>> => {
      try {
        secureStore.preferences = {
          ...secureStore.preferences,
          ...preferences,
        };
        saveConfig();
        startRecordingBridge("save-preferences");

        return { success: true, data: undefined };
      } catch (error) {
        console.error("Error saving preferences:", error);
        return { success: false, error: String(error) };
      }
    },
  );

  // Load preferences
  ipcMain.handle(
    "load-preferences",
    async (): Promise<IpcResponse<Preferences>> => {
      try {
        return {
          success: true,
          data: { ...DEFAULT_PREFERENCES, ...secureStore.preferences },
        };
      } catch (error) {
        console.error("Error loading preferences:", error);
        return { success: false, error: String(error) };
      }
    },
  );

  // Close settings window
  ipcMain.handle("close-settings", async () => {
    if (settingsWindow) {
      settingsWindow.close();
    }
    return true;
  });

  // Authentication completed
  ipcMain.handle("authentication-completed", async () => {
    updateTrayMenu();

    // Close main window if it exists
    if (mainWindow) {
      mainWindow.close();
    }

    return true;
  });

  // Resize window for consent page
  ipcMain.handle("resize-for-consent", async () => {
    if (mainWindow) {
      mainWindow.setSize(760, 700);
      mainWindow.center();
    }
    return true;
  });

  // Resize window for API key page
  ipcMain.handle("resize-for-api-key", async () => {
    if (mainWindow) {
      mainWindow.setSize(440, 380);
      mainWindow.center();
    }
    return true;
  });

  // Start upload with progress tracking
  ipcMain.handle("start-upload-with-progress", async (_, options) => {
    try {
      console.log("Starting upload with progress tracking");

      const uploadProcess = spawnUv(
        [
          "run",
          "-m",
          "vg_control.upload_bridge",
          "--api-token",
          options.apiToken,
          "--progress", // Add progress flag for detailed output
          ...(options.unreliableConnection ? ["--unreliable-connections"] : []),
        ],
        {
          cwd: rootDir(),
        },
      );

      const processId = uploadProcess.pid;
      let finalStats = {
        totalFiles: 0,
        filesUploaded: 0,
        totalDuration: 0,
        totalBytes: 0,
      };

      // Handle progress output from Python
      uploadProcess.stdout.on("data", (data: Buffer) => {
        const output = data.toString();
        console.log(`Upload stdout: ${output}`);

        // Parse progress information from Python output
        // Expected format: "PROGRESS: {json_data}" or "FINAL_STATS: {json_data}"
        const lines = output.split("\n");
        for (const line of lines) {
          if (line.startsWith("PROGRESS: ")) {
            try {
              const progressData = JSON.parse(line.substring(10));
              // Send progress to renderer
              if (settingsWindow) {
                settingsWindow.webContents.send(
                  "upload-progress",
                  progressData,
                );
              }
              if (mainWindow) {
                mainWindow.webContents.send("upload-progress", progressData);
              }
            } catch (e) {
              console.error("Failed to parse progress data:", e);
            }
          } else if (line.startsWith("FINAL_STATS: ")) {
            try {
              const statsData = JSON.parse(line.substring(13));
              finalStats = {
                totalFiles: statsData.total_files_uploaded || 0,
                filesUploaded: statsData.total_files_uploaded || 0,
                totalDuration: statsData.total_duration_uploaded || 0,
                totalBytes: statsData.total_bytes_uploaded || 0,
              };
              console.log("Captured final stats:", finalStats);
            } catch (e) {
              console.error("Failed to parse final stats:", e);
            }
          }
        }
      });

      uploadProcess.stderr.on("data", (data: Buffer) => {
        console.error(`Upload stderr: ${data.toString()}`);
      });

      uploadProcess.on("close", (code: number) => {
        console.log(`Upload process exited with code ${code}`);

        // Send completion message with captured stats
        const completionData = {
          success: code === 0,
          code: code,
          totalFiles: finalStats.totalFiles,
          filesUploaded: finalStats.filesUploaded,
          totalDuration: finalStats.totalDuration,
          totalBytes: finalStats.totalBytes,
        };

        if (settingsWindow) {
          settingsWindow.webContents.send("upload-complete", completionData);
        }
        if (mainWindow) {
          mainWindow.webContents.send("upload-complete", completionData);
        }
      });

      return { success: true, processId: processId };
    } catch (error) {
      console.error("Error starting upload with progress:", error);
      return { success: false, error: String(error) };
    }
  });

  // Stop upload process
  ipcMain.handle("stop-upload-process", async (_, processId) => {
    try {
      if (processId) {
        process.kill(processId, "SIGTERM");
        return { success: true };
      }
      return { success: false, error: "No process ID provided" };
    } catch (error) {
      console.error("Error stopping upload process:", error);
      return { success: false, error: String(error) };
    }
  });
}
