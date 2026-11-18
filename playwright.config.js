// Headless WebKit runs of the scripts cooee injects into Meet (src-tauri/inject/), against
// fixture pages served as https://meet.google.com. Headless: no window ever takes focus.
const { defineConfig, devices } = require("@playwright/test");

module.exports = defineConfig({
  testDir: "tests/inject",
  forbidOnly: true,
  reporter: "line",
  timeout: 20000,
  projects: [{ name: "webkit", use: { ...devices["Desktop Safari"], headless: true } }],
});
