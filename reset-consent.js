// Script to reset the OWL Control app's consent status
// This will make the consent screen appear again when you restart the app

const fs = require("fs");
const path = require("path");
const os = require("os");

// Determine userDataPath based on platform
const platform = process.platform;
const appName = "vg-control"; // Changed from vg_control to vg-control
let userDataPath;

if (platform === "win32") {
  userDataPath = path.join(process.env.APPDATA, appName);
} else if (platform === "darwin") {
  userDataPath = path.join(
    os.homedir(),
    "Library",
    "Application Support",
    appName,
  );
} else {
  // Linux and others
  userDataPath = path.join(os.homedir(), ".config", appName);
}

const configPath = path.join(userDataPath, "config.json");

console.log(`Looking for config file at: ${configPath}`);

// Check if config file exists
if (fs.existsSync(configPath)) {
  // Read the current config
  try {
    const config = JSON.parse(fs.readFileSync(configPath, "utf8"));

    console.log("Current config:", JSON.stringify(config, null, 2));

    // Modify the hasConsented value to false
    if (config.credentials) {
      config.credentials.hasConsented = false; // Using boolean instead of string

      // Write the modified config back
      fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
      console.log("Successfully reset consent status to false");
    } else {
      console.log("Config file does not contain credentials section");
    }
  } catch (error) {
    console.error("Error modifying config file:", error);
  }
} else {
  console.log(
    "Config file not found. The app may not have been run yet or is using a different location.",
  );
}

// Reminder for the user
console.log(
  "\nRemember to restart the OWL Control app for changes to take effect.",
);
console.log("When you restart, you should see the consent screen again.");
