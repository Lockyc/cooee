// cooee's faces window. Runs in the Meet page but knows nothing about Meet: it consumes only the
// adapter's neutral model (window.__cooee) and renders into a popup that Rust turns into the
// always-on-top faces window (src-tauri/src/faces.rs).
(function () {
  if (window.top !== window || location.host !== "meet.google.com") return;

  // How a participant's video reaches the popup. "stream": share the MediaStream (same WebKit
  // process). "canvas": draw frames from a hidden <video> here into a popup <canvas> — the
  // fallback if a shared stream won't render in the popup.
  const RENDER = "stream";
  const PULSE_MS = 600;

  const ipc = (cmd, args) => {
    try {
      return window.__TAURI_INTERNALS__?.invoke(cmd, args);
    } catch (_) {}
  };

  const CSS = `
    :root { color-scheme: dark; --bg:#111; --fg:#eee; --muted:#9aa; --accent:#f6c945; --chip:#2a2f33; }
    * { box-sizing: border-box; }
    html, body { margin:0; height:100%; background:var(--bg); color:var(--fg); font:13px -apple-system, system-ui, sans-serif; overflow:hidden; }
    #root { position:absolute; inset:0; display:flex; flex-direction:column; }
    #grid { flex:1; display:grid; gap:4px; padding:4px; min-height:0; }
    .tile { position:relative; background:#000; border-radius:8px; overflow:hidden; outline:3px solid transparent; outline-offset:-3px; transition:outline-color .15s; }
    .tile.speaking { outline-color:var(--accent); }
    .tile video, .tile canvas { width:100%; height:100%; object-fit:cover; display:block; }
    .tile .name { position:absolute; left:6px; bottom:5px; padding:1px 6px; border-radius:4px; background:rgba(0,0,0,.55); font-size:11px; }
    #chips { display:flex; gap:4px; padding:0 4px 4px; flex-wrap:wrap; }
    #chips:empty { display:none; }
    .chip { padding:2px 8px; border-radius:10px; background:var(--chip); color:var(--muted); font-size:11px; border:1px solid transparent; }
    .chip.speaking { color:var(--fg); border-color:var(--accent); }
    #message { position:absolute; inset:0; display:none; align-items:center; justify-content:center; text-align:center; padding:16px; color:var(--muted); font-size:14px; }
    #message.show { display:flex; }
    #bar { position:absolute; left:50%; bottom:8px; transform:translateX(-50%); display:flex; gap:4px; padding:4px; border-radius:10px; background:rgba(20,22,24,.85); opacity:0; transition:opacity .15s; }
    body:hover #bar { opacity:1; }
    #bar button { all:unset; cursor:pointer; padding:4px 8px; border-radius:6px; font-size:12px; color:var(--fg); }
    #bar button:hover { background:#333; }
    #bar .leave { color:#ff6b5e; }
    #bar .sep { width:1px; background:#444; margin:2px 2px; }
    body.pulse #root { animation: pulse ${PULSE_MS}ms ease-out; }
    @keyframes pulse { 0% { box-shadow: inset 0 0 0 4px var(--accent); } 100% { box-shadow: inset 0 0 0 0 transparent; } }
  `;

  const CONTROLS = [
    ["Mute", () => window.__cooee.act("mic")],
    ["Camera", () => window.__cooee.act("camera")],
    ["Leave", () => window.__cooee.act("leave"), "leave"],
    null,
    ["Talk", () => ipc("snap_talk")],
    ["Aside", () => ipc("snap_aside")],
    ["Size", () => ipc("cycle_size")],
    null,
    ["Save Talk", () => ipc("save_talk")],
    ["Save Aside", () => ipc("save_aside")],
  ];

  let popup = null;
  let ui = null;
  const tiles = new Map(); // participant id → { el, media, video?, stream }
  let lastOthers = 0;

  function build(d) {
    d.title = "cooee";
    const style = d.createElement("style");
    style.textContent = CSS;
    d.head.append(style);
    d.body.innerHTML = '<div id="root"><div id="grid"></div><div id="chips"></div></div><div id="message"></div><div id="bar"></div>';
    const bar = d.getElementById("bar");
    for (const c of CONTROLS) {
      if (!c) {
        bar.append(Object.assign(d.createElement("span"), { className: "sep" }));
        continue;
      }
      const b = d.createElement("button");
      b.textContent = c[0];
      if (c[2]) b.className = c[2];
      b.onclick = c[1];
      bar.append(b);
    }
    return { d, grid: d.getElementById("grid"), chips: d.getElementById("chips"), message: d.getElementById("message") };
  }

  function mediaFor(d, stream) {
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

  function render(model) {
    if (!popup || popup.closed || !ui) return;
    const { d, grid, chips, message } = ui;
    const others = model.participants.filter((p) => !p.isSelf && !p.isPresentation);
    const faces = others.filter((p) => p.cameraOn && p.stream);
    const off = others.filter((p) => !(p.cameraOn && p.stream));

    // Tiles: reuse each participant's element; only a new stream replaces its media.
    const keep = new Set(faces.map((p) => p.id));
    for (const [id, t] of tiles) {
      if (!keep.has(id)) {
        t.el.remove();
        tiles.delete(id);
      }
    }
    for (const p of faces) {
      let t = tiles.get(p.id);
      if (!t || t.stream !== p.stream) {
        if (t) t.el.remove();
        const el = d.createElement("div");
        el.className = "tile";
        const { media, video } = mediaFor(d, p.stream);
        const name = Object.assign(d.createElement("span"), { className: "name" });
        el.append(media, name);
        t = { el, media, video, stream: p.stream, name };
        tiles.set(p.id, t);
      }
      t.name.textContent = p.name;
      t.el.classList.toggle("speaking", p.speaking);
      grid.append(t.el); // re-append keeps participant order
    }
    const cols = Math.max(1, Math.ceil(Math.sqrt(faces.length)));
    grid.style.gridTemplateColumns = `repeat(${cols}, 1fr)`;
    grid.style.gridTemplateRows = `repeat(${Math.max(1, Math.ceil(faces.length / cols))}, 1fr)`;

    chips.replaceChildren(
      ...off.map((p) => Object.assign(d.createElement("span"), { className: "chip" + (p.speaking ? " speaking" : ""), textContent: p.name || "Guest" }))
    );

    let text = "";
    if (!model.adapterOk) text = "Meet's layout changed. cooee can't find the video tiles.";
    else if (others.length === 0) text = "Waiting for others";
    message.textContent = text;
    message.classList.toggle("show", !!text);

    if (others.length > lastOthers) {
      d.body.classList.remove("pulse");
      void d.body.offsetWidth;
      d.body.classList.add("pulse");
      setTimeout(() => d.body.classList.remove("pulse"), PULSE_MS);
    }
    lastOthers = others.length;
  }

  function open() {
    if (popup && !popup.closed) return;
    popup = window.open("", "cooee-faces", "width=560,height=350");
    if (!popup) return;
    tiles.clear();
    lastOthers = 0;
    ui = build(popup.document);
    render(window.__cooee.model());
  }

  function onModel(model) {
    if (model.inCall) open();
    else {
      popup = null;
      ui = null;
      tiles.clear();
    }
    render(model);
  }

  function start() {
    if (!window.__cooee) return setTimeout(start, 200);
    window.__cooee.subscribe(onModel);
    onModel(window.__cooee.model());
  }

  // Rust reopens the popup through this when the faces window was closed mid-call.
  window.__cooeeFaces = { open: () => window.__cooee?.model().inCall && open() };
  start();
})();
