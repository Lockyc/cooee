// Loads a fixture page as https://meet.google.com with cooee's injected scripts added the way
// meet.rs adds them (document-start, in INJECTED_JS order), over two stubs installed first:
// - window.__TAURI_INTERNALS__.invoke records every IPC call in window.__ipc and rejects the next
//   N calls of a command after window.__fake.rejectNext(cmd, n);
// - MediaDevices.prototype.getUserMedia / getDisplayMedia resolve to canvas-captured streams, and
//   window.__fake.setMuted(track, bool) mutes a track the way a network freeze does.
const fs = require("fs");
const path = require("path");

const INJECT = path.join(__dirname, "../../src-tauri/inject");
const INJECTED = ["meet-adapter.js", "popup.js", "faces.js", "controls.js", "share.js"];
const FIXTURES = path.join(__dirname, "fixtures");

// Meet's own policy, which every popup inherits.
const TRUSTED_TYPES_CSP = "require-trusted-types-for 'script'";

function stubs() {
  window.__ipc = [];
  const rejecting = new Map();
  window.__TAURI_INTERNALS__ = {
    invoke(cmd, args) {
      window.__ipc.push({ cmd, args: args === undefined ? null : JSON.parse(JSON.stringify(args)) });
      const n = rejecting.get(cmd) || 0;
      if (n > 0) {
        rejecting.set(cmd, n - 1);
        return Promise.reject(new Error(`stub: ${cmd} rejected`));
      }
      return Promise.resolve(null);
    },
  };

  const fakeStream = (label) => {
    const c = document.createElement("canvas");
    c.width = 320;
    c.height = 180;
    const ctx = c.getContext("2d");
    ctx.fillStyle = "#3a6";
    ctx.fillRect(0, 0, 320, 180);
    ctx.fillStyle = "#fff";
    ctx.fillText(label, 10, 20);
    const stream = c.captureStream(30);
    // Keep frames flowing so a <video> playing the stream has a size.
    const paint = () => {
      ctx.fillRect(Math.random() * 300, 30, 2, 2);
      requestAnimationFrame(paint);
    };
    requestAnimationFrame(paint);
    return stream;
  };
  const muted = new WeakMap();
  window.__fake = {
    stream: fakeStream,
    rejectNext: (cmd, n = 1) => rejecting.set(cmd, n),
    setMuted(track, on) {
      if (!muted.has(track)) {
        muted.set(track, false);
        Object.defineProperty(track, "muted", { get: () => muted.get(track) });
      }
      muted.set(track, on);
    },
  };
  MediaDevices.prototype.getUserMedia = async function () {
    return fakeStream("camera");
  };
  MediaDevices.prototype.getDisplayMedia = async function () {
    return fakeStream("screen");
  };
}

// Open `fixture` (a file in fixtures/) as a Meet call page. `csp` adds a
// Content-Security-Policy header; `clock` installs Playwright's fake clock first.
async function openMeet(page, fixture, { csp, clock } = {}) {
  const body = fs.readFileSync(path.join(FIXTURES, fixture), "utf8");
  await page.route("https://meet.google.com/**", (route) =>
    route.fulfill({
      status: 200,
      contentType: "text/html",
      headers: csp ? { "content-security-policy": csp } : {},
      body,
    })
  );
  if (clock) await page.clock.install();
  await page.addInitScript(stubs);
  for (const f of INJECTED) {
    await page.addInitScript({ content: fs.readFileSync(path.join(INJECT, f), "utf8") });
  }
  await page.goto("https://meet.google.com/abc-defg-hij");
}

const ipcCalls = (page, cmd) =>
  page.evaluate((cmd) => window.__ipc.filter((c) => c.cmd === cmd).map((c) => c.args), cmd);

// The model without its MediaStreams (which don't cross into the test).
const model = (page) =>
  page.evaluate(() => {
    const m = window.__cooee.model();
    return {
      ...m,
      participants: m.participants.map(({ stream, ...p }) => ({ ...p, stream: !!stream })),
      self: { ...m.self, stream: !!m.self.stream },
      presentation: m.presentation && { id: m.presentation.id, name: m.presentation.name },
    };
  });

module.exports = { openMeet, ipcCalls, model, TRUSTED_TYPES_CSP };
