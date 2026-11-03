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

  const ipc = (cmd, args) => {
    try {
      return window.__TAURI_INTERNALS__?.invoke(cmd, args);
    } catch (_) {}
  };

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
      const cameraOn = visibleVideo(video);
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
    const r = JSON.stringify([model.inCall, others, model.adapterOk]);
    if (r === lastReported) return;
    lastReported = r;
    ipc("meet_state", { inCall: model.inCall, others, adapterOk: model.adapterOk });
  }

  let queued = false;
  function refresh() {
    queued = false;
    const next = read();
    const s = signature(next);
    if (s === sig) return;
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
          { at: new Date().toISOString(), model: { ...model, participants }, tiles, buttons },
          null,
          2
        ),
      });
    },
  };

  if (document.body) start();
  else document.addEventListener("DOMContentLoaded", start, { once: true });
})();
