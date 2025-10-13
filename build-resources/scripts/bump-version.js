#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

// Helper function to update version in TOML files using regex
function updateTomlVersion(filePath, newVersion) {
  const content = fs.readFileSync(filePath, "utf8");
  // Only replace the first occurrence of version = "..." (the package version)
  const updatedContent = content.replace(
    /version\s*=\s*"[^"]+"/,
    `version = "${newVersion}"`
  );
  fs.writeFileSync(filePath, updatedContent);
  console.log(`Updated version in ${path.basename(filePath)} to ${newVersion}`);
}

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

// Update Cargo.toml
const cargoTomlPath = path.resolve(
  __dirname,
  "..",
  "crates",
  "owl-recorder",
  "Cargo.toml"
);
updateTomlVersion(cargoTomlPath, newVersion);

console.log(`Version bumped from ${currentVersion} to ${newVersion}`);

try {
  // Update lock files
  console.log("Updating lock files...");
  execSync("cargo check", { stdio: "inherit" });

  // Commit the version change
  const filesToAdd = ["crates/owl-recorder/Cargo.toml", "Cargo.lock"];
  execSync(`git add ${filesToAdd.join(" ")}`);
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
