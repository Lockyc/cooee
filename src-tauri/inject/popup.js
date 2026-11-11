// cooee's popup lifecycle, shared by the faces, controls and share windows (faces.js,
// controls.js, share.js). Each registers a kind with `__cooeePopup`; this opens it as
// `about:blank#<kind>` while the adapter's model wants it, (re)builds its UI into whatever
// document the popup holds, and re-renders on every model change. Rust turns the popup into an
// always-on-top window (src-tauri/src/popups.rs) and destroys it when the call state no longer
// allows it. Knows nothing about Meet: it consumes only window.__cooee.
(function () {
  if (window.top !== window || location.host !== "meet.google.com") return;

  // How a video reaches a popup. "stream": share the MediaStream (same WebKit process).
  // "canvas": draw frames from a hidden <video> here into a popup <canvas> — the fallback if a
  // shared stream won't render in a popup.
  const RENDER = "stream";

  const BASE_CSS = `
    :root { color-scheme: dark; --bg:#111; --fg:#eee; --muted:#9aa; --accent:#f6c945; --chip:#2a2f33; --danger:#ff6b5e; }
    * { box-sizing: border-box; }
    html, body { margin:0; height:100%; background:var(--bg); color:var(--fg); font:13px -apple-system, system-ui, sans-serif; overflow:hidden; }
    #root { position:absolute; inset:0; }
  `;

  // A <video> (or canvas) playing `stream` in document `d`.
  function media(d, stream) {
    if (RENDER === "stream") {
      const v = d.createElement("video");
      Object.assign(v, { autoplay: true, muted: true, playsInline: true });
      v.srcObject = stream;
      v.play().catch(() => {});
      return { media: v };
    }
    const source = document.createElement("video");
    Object.assign(source, { autoplay: true, muted: true, playsInline: true });
    source.srcObject = stream;
    source.play().catch(() => {});
    const c = d.createElement("canvas");
    const ctx = c.getContext("2d");
    const draw = () => {
      if (!c.isConnected) return;
      if (source.videoWidth) {
        if (c.width !== source.videoWidth) c.width = source.videoWidth;
        if (c.height !== source.videoHeight) c.height = source.videoHeight;
        ctx.drawImage(source, 0, 0);
      }
      source.requestVideoFrameCallback(draw);
    };
    source.requestVideoFrameCallback(draw);
    return { media: c, video: source };
  }

  // Detach a media's stream so a removed element (or the canvas mode's hidden source) stops
  // decoding.
  function release(m) {
    m.media.remove();
    if (m.media.srcObject) m.media.srcObject = null;
    if (m.video) {
      m.video.pause();
      m.video.srcObject = null;
    }
  }

  // Media diagnostics for the layout snapshot.
  function mediaDiag(m, stream) {
    const v = m.video || m.media;
    const track = stream?.getVideoTracks?.()[0];
    return {
      connected: m.media.isConnected,
      box: [m.media.clientWidth, m.media.clientHeight],
      readyState: v.readyState,
      paused: v.paused,
      videoSize: [v.videoWidth, v.videoHeight],
      track: track && { readyState: track.readyState, muted: track.muted, enabled: track.enabled },
    };
  }

  const kinds = new Map(); // kind → controller

  // spec: { size: [w, h], css, build(d) → ui (with ui.root), render(ui, model), wanted(model),
  //         reset?(), diag?() }
  function register(kind, spec) {
    let popup = null;
    let ui = null;

    function open() {
      if (popup && !popup.closed) return;
      // An explicit URL: WebKit reports `window.open("")` as an empty string, which Tauri can't
      // parse and denies; the fragment tells Rust which window this is.
      const [w, h] = spec.size;
      popup = window.open(`about:blank#${kind}`, `cooee-${kind}`, `width=${w},height=${h}`);
      if (popup) ensureBuilt();
    }

    // Tauri's window loads about:blank itself once created, replacing the document `open()`
    // first sees, so the UI is (re)built into whichever document the popup holds now. Built with
    // DOM calls, never innerHTML: the popup inherits Meet's `require-trusted-types-for 'script'`
    // CSP, under which an innerHTML string throws.
    function ensureBuilt() {
      if (!popup || popup.closed) return;
      const d = popup.document;
      if (ui && ui.d === d && d.getElementById("root")) return;
      spec.reset?.();
      const style = d.createElement("style");
      style.textContent = BASE_CSS + spec.css;
      d.head.replaceChildren(style);
      d.title = "cooee";
      ui = { ...spec.build(d), d };
      ui.root.id = "root";
      d.body.replaceChildren(ui.root);
      spec.render(ui, window.__cooee.model());
    }

    function onModel(model) {
      if (spec.wanted(model)) open();
      else if (popup) {
        // Rust destroys the window; forget it so the next want opens a fresh one.
        popup = null;
        ui = null;
        spec.reset?.();
      }
      if (ui && popup && !popup.closed) spec.render(ui, model);
    }

    function diag() {
      let doc = null;
      try {
        const d = popup?.document;
        doc = d && {
          url: d.URL,
          readyState: d.readyState,
          built: !!ui && ui.d === d && !!d.getElementById("root"),
          title: d.title,
        };
      } catch (e) {
        doc = { error: String(e) };
      }
      return { popupOpen: !!popup && !popup.closed, doc, ...(spec.diag?.() || {}) };
    }

    kinds.set(kind, {
      open: () => window.__cooee && spec.wanted(window.__cooee.model()) && open(),
      onModel,
      ensureBuilt,
      diag,
    });
  }

  function start() {
    if (!window.__cooee) return setTimeout(start, 200);
    for (const k of kinds.values()) {
      window.__cooee.subscribe(k.onModel);
      k.onModel(window.__cooee.model());
    }
    setInterval(() => kinds.forEach((k) => k.ensureBuilt()), 250);
  }

  window.__cooeePopup = register;
  window.__cooeeMedia = { media, release, diag: mediaDiag };
  window.__cooeePopups = {
    // Rust reopens a popup through this when its window was closed mid-call.
    open: (kind) => kinds.get(kind)?.open(),
    // Diagnostics, saved in the adapter's snapshot.
    diag: () => Object.fromEntries([...kinds].map(([kind, k]) => [kind, k.diag()])),
  };
  // The popup scripts register synchronously after this one; start once they have.
  setTimeout(start, 0);
})();
