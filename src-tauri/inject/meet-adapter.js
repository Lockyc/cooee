// cooee's Meet adapter — the ONLY code that knows Google Meet's DOM. It reads the call into a
// platform-neutral model ({ inCall, adapterOk, participants: [...] }) and performs Meet's own
// controls. Everything else in cooee consumes `window.__cooee` and never touches Meet's DOM, so a
// Meet redesign is fixed here, in SELECTORS and the readers below it.
(function () {
  if (window.top !== window || location.host !== "meet.google.com") return;
  if (window.__cooee) return;

  // Meet's markers. aria-labels are English-UI strings; a non-English UI needs its own set.
  const SELECTORS = {
    leave: 'button[aria-label^="Leave call" i]',
    mic: 'button[aria-label*="microphone" i][aria-label^="Turn" i]',
    camera: 'button[aria-label*="camera" i][aria-label^="Turn" i]',
    // One element per rendered tile; a participant's id is stable for the call.
    tile: "[data-participant-id]",
    tileId: "data-participant-id",
    // Name text inside a tile, most specific first.
    name: ["[data-self-name]", "[data-tooltip]", ".notranslate", "span"],
    // A tile whose label or text says it is a presentation.
    presentation: /present/i,
    // Self: Meet marks the local tile, and mirrors its video.
    selfAttr: "data-self-name",
    selfText: /^\(?you\)?$/i,
    // The active-speaker cue on a tile (calibrated against a live call; empty = unknown).
    speaking: "",
  };
  const HEALTH_GRACE_MS = 10000;

  // Local camera/mic tracks, captured by wrapping getUserMedia before Meet's scripts run. This
  // is the selector-free signal that a call (or its preview) is live, so a join never silently
  // drops a call the DOM readers below have lost track of.
  const localTracks = new Set();
  const md = navigator.mediaDevices;
  if (md && md.getUserMedia) {
    const gum = md.getUserMedia.bind(md);
    md.getUserMedia = async (constraints) => {
      const stream = await gum(constraints);
      for (const t of stream.getTracks()) {
        localTracks.add(t);
        t.addEventListener("ended", () => localTracks.delete(t));
      }
      return stream;
    };
  }
  const capturing = () => [...localTracks].some((t) => t.readyState === "live");

  // Always a promise, so a caller can retry on failure.
  const ipc = (cmd, args) => {
    try {
      const p = window.__TAURI_INTERNALS__?.invoke(cmd, args);
      return p || Promise.reject(new Error("no Tauri IPC"));
    } catch (e) {
      return Promise.reject(e);
    }
  };

  // A tile's video stays "on" through a brief freeze (a network blip mutes the track for a
  // moment), so the faces window doesn't flip it to a name chip and back.
  const CAMERA_OFF_GRACE_MS = 1500;
  const lastLive = new Map(); // participant id → last time its video was live

  function visibleVideo(v) {
    if (!v || !v.srcObject || v.videoWidth === 0) return false;
    const track = v.srcObject.getVideoTracks?.()[0];
    if (!track || track.readyState !== "live" || track.muted) return false;
    return v.offsetParent !== null && getComputedStyle(v).visibility !== "hidden";
  }

  function tileName(tile) {
    for (const sel of SELECTORS.name) {
      const el = tile.querySelector(sel);
      const t = (el?.getAttribute(SELECTORS.selfAttr) || el?.textContent || "").trim();
      if (t) return t;
    }
    return "";
  }

  function isSelf(tile, video) {
    if (tile.querySelector(`[${SELECTORS.selfAttr}]`)) return true;
    const leaves = [...tile.querySelectorAll("span,div")].filter((e) => e.children.length === 0);
    if (leaves.some((e) => SELECTORS.selfText.test(e.textContent.trim()))) return true;
    return !!video && /matrix\(-1|scaleX\(-1\)/.test(getComputedStyle(video).transform);
  }

  function readParticipants() {
    const byId = new Map();
    for (const tile of document.querySelectorAll(SELECTORS.tile)) {
      const id = tile.getAttribute(SELECTORS.tileId);
      if (!id || byId.has(id)) continue;
      const video = tile.querySelector("video");
      const label = `${tile.getAttribute("aria-label") || ""} ${tile.textContent.slice(0, 200)}`;
      const now = Date.now();
      if (visibleVideo(video)) lastLive.set(id, now);
      const cameraOn = !!video?.srcObject && now - (lastLive.get(id) || 0) < CAMERA_OFF_GRACE_MS;
      byId.set(id, {
        id,
        name: tileName(tile),
        isSelf: isSelf(tile, video),
        isPresentation: SELECTORS.presentation.test(label),
        cameraOn,
        speaking: !!SELECTORS.speaking && !!tile.querySelector(SELECTORS.speaking),
        stream: cameraOn ? video.srcObject : null,
      });
    }
    return [...byId.values()];
  }

  let inCallSince = 0;
  function read() {
    const inCall = !!document.querySelector(SELECTORS.leave);
    const participants = inCall ? readParticipants() : [];
    if (!inCall) inCallSince = 0;
    else if (!inCallSince) inCallSince = Date.now();
    const adapterOk =
      !inCall || participants.length > 0 || Date.now() - inCallSince < HEALTH_GRACE_MS;
    return { inCall, adapterOk, participants };
  }

  const signature = (m) =>
    JSON.stringify([
      m.inCall,
      m.adapterOk,
      m.participants.map((p) => [p.id, p.name, p.isSelf, p.isPresentation, p.cameraOn, p.speaking, p.stream?.id]),
    ]);

  let model = read();
  let sig = signature(model);
  const listeners = new Set();
  let lastReported = "";

  function report() {
    const others = model.participants.filter((p) => !p.isSelf && !p.isPresentation).length;
    const live = capturing();
    const r = JSON.stringify([model.inCall, others, model.adapterOk, live]);
    if (r === lastReported) return;
    lastReported = r;
    ipc("meet_state", { inCall: model.inCall, others, adapterOk: model.adapterOk, capturing: live }).catch(() => {
      if (lastReported === r) lastReported = ""; // resend on the next tick
    });
  }

  let queued = false;
  function refresh() {
    queued = false;
    const next = read();
    const s = signature(next);
    if (s === sig) {
      report(); // capture state changes without a model change; report() dedups
      return;
    }
    model = next;
    sig = s;
    report();
    for (const fn of listeners) {
      try {
        fn(model);
      } catch (e) {
        console.error("cooee listener", e);
      }
    }
  }
  const schedule = () => {
    if (!queued) {
      queued = true;
      setTimeout(refresh, 250);
    }
  };

  function start() {
    new MutationObserver(schedule).observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["aria-label", "class", "style", SELECTORS.tileId],
    });
    // Video tracks mute/unmute without DOM changes, and the health grace needs a clock.
    setInterval(schedule, 1000);
    report();
  }

  window.__cooee = {
    model: () => model,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    act(kind) {
      const el = document.querySelector(SELECTORS[kind]);
      if (el) el.click();
      return !!el;
    },
    // Diagnostics: the tiles' structure (no media, no page text beyond names) for recalibrating
    // SELECTORS after a Meet redesign. Saved by the tray's "Save Meet layout snapshot".
    dump() {
      const tiles = [...document.querySelectorAll(SELECTORS.tile)].map((t) => ({
        id: t.getAttribute(SELECTORS.tileId),
        ariaLabel: t.getAttribute("aria-label"),
        attrs: [...t.attributes].map((a) => a.name),
        html: t.outerHTML.replace(/>[^<]{40,}</g, ">…<").slice(0, 4000),
      }));
      const buttons = [...document.querySelectorAll("button[aria-label]")].map((b) =>
        b.getAttribute("aria-label")
      );
      const participants = model.participants.map(({ stream, ...p }) => p);
      ipc("adapter_dump", {
        json: JSON.stringify(
          {
            at: new Date().toISOString(),
            model: { ...model, participants },
            tiles,
            buttons,
            faces: window.__cooeeFaces?.diag?.() ?? null,
          },
          null,
          2
        ),
      }).catch((e) => console.error("cooee snapshot", e));
    },
  };

  if (document.body) start();
  else document.addEventListener("DOMContentLoaded", start, { once: true });
})();
