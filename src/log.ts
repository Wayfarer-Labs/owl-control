import path from "path";
import * as os from "os";
import * as fs from "fs";

// Set up file logging
const logFilePath = path.join(os.tmpdir(), "owl-control-debug.log");
export function logToFile(message: string) {
  const timestamp = new Date().toISOString();
  const logLine = `[${timestamp}] ${message}\n`;
  try {
    fs.appendFileSync(logFilePath, logLine);
  } catch (e) {
    // If we can't write to temp, try current directory
    try {
      fs.appendFileSync("owl-control-debug.log", logLine);
    } catch (e2) {
      // Give up
    }
  }
}

// Override console methods to also log to file
const originalConsoleLog = console.log;
const originalConsoleError = console.error;
console.log = (...args) => {
  const message = args.join(" ");
  logToFile(`LOG: ${message}`);
  originalConsoleLog(...args);
};
console.error = (...args) => {
  const message = args.join(" ");
  logToFile(`ERROR: ${message}`);
  originalConsoleError(...args);
};
