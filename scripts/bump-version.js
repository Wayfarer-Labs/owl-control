#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

// Argument parsing
const args = process.argv.slice(2);
const versionType = args[0] || "patch"; // Default to patch version bump

if (!["major", "minor", "patch"].includes(versionType)) {
  console.error("Invalid version type. Use: major, minor, or patch");
  process.exit(1);
}

// Read the current package.json
const packageJsonPath = path.resolve(__dirname, "..", "package.json");
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
const currentVersion = packageJson.version;

// Parse version
const [major, minor, patch] = currentVersion.split(".").map(Number);

// Calculate new version
let newVersion;
switch (versionType) {
  case "major":
    newVersion = `${major + 1}.0.0`;
    break;
  case "minor":
    newVersion = `${major}.${minor + 1}.0`;
    break;
  case "patch":
    newVersion = `${major}.${minor}.${patch + 1}`;
    break;
}

// Update package.json
packageJson.version = newVersion;
fs.writeFileSync(packageJsonPath, JSON.stringify(packageJson, null, 2) + "\n");

console.log(`Version bumped from ${currentVersion} to ${newVersion}`);

try {
  // Commit the version change
  execSync("git add package.json");
  execSync(`git commit -m "Bump version to ${newVersion}"`);

  // Create a tag
  execSync(`git tag v${newVersion}`);

  console.log(`
Version bump complete!

To push the changes and trigger the build workflow, run:
  git push && git push --tags
`);
} catch (error) {
  console.error("Error during git operations:", error.message);
  process.exit(1);
}
