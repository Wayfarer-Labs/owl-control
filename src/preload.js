// Preload script runs in a privileged context with access to Node.js APIs
window.addEventListener("DOMContentLoaded", () => {
  // Expose useful APIs to the renderer process
  const replaceText = (selector, text) => {
    const element = document.getElementById(selector);
    if (element) element.innerText = text;
  };

  // Report app version
  for (const type of ["chrome", "node", "electron"]) {
    const version = process.versions[type];
    replaceText(`${type}-version`, version);
  }
});
