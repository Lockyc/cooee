// cooee's faces window: the other people's faces, and nothing else. Knows nothing about Meet: it
// renders the adapter's neutral model (window.__cooee) into the popup popup.js opens.
(function () {
  if (window.top !== window || location.host !== "meet.google.com") return;

  const PULSE_MS = 600;
  const { media, release, diag: mediaDiag } = window.__cooeeMedia;

  const CSS = `
    #root { display:flex; flex-direction:column; }
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
    body.pulse #root { animation: pulse ${PULSE_MS}ms ease-out; }
    @keyframes pulse { 0% { box-shadow: inset 0 0 0 4px var(--accent); } 100% { box-shadow: inset 0 0 0 0 transparent; } }
  `;

  const tiles = new Map(); // participant id → { el, name, stream, m }
  let lastOthers = 0;

  function drop(t) {
    release(t.m);
    t.el.remove();
  }

  function build(d) {
    const div = (id) => Object.assign(d.createElement("div"), { id });
    const grid = div("grid");
    const chips = div("chips");
    const message = div("message");
    const root = d.createElement("div");
    root.append(grid, chips, message);
    return { root, grid, chips, message };
  }

  function render(ui, model) {
    const { d, grid, chips, message } = ui;
    const others = model.participants.filter((p) => !p.isSelf && !p.isPresentation);
    const faces = others.filter((p) => p.cameraOn && p.stream);
    const off = others.filter((p) => !(p.cameraOn && p.stream));

    // Tiles: reuse each participant's element; only a new stream replaces its media.
    const keep = new Set(faces.map((p) => p.id));
    for (const [id, t] of tiles) {
      if (!keep.has(id)) {
        drop(t);
        tiles.delete(id);
      }
    }
    for (const p of faces) {
      let t = tiles.get(p.id);
      if (!t || t.stream !== p.stream) {
        if (t) drop(t);
        const el = d.createElement("div");
        el.className = "tile";
        const m = media(d, p.stream);
        const name = Object.assign(d.createElement("span"), { className: "name" });
        el.append(m.media, name);
        t = { el, name, stream: p.stream, m };
        tiles.set(p.id, t);
      }
      t.name.textContent = p.name;
      t.el.classList.toggle("speaking", p.speaking);
    }
    // Re-order only when the order changed: moving a <video> in the DOM can make WebKit tear
    // down and rebuild its render layer (a visible flicker).
    const order = faces.map((p) => tiles.get(p.id).el);
    if (order.length !== grid.children.length || order.some((el, i) => grid.children[i] !== el)) {
      grid.replaceChildren(...order);
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

  window.__cooeePopup("faces", {
    css: CSS,
    build,
    render,
    wanted: (model) => model.inCall,
    reset() {
      for (const t of tiles.values()) drop(t);
      tiles.clear();
      lastOthers = 0;
    },
    diag: () => ({
      tiles: [...tiles].map(([id, t]) => ({ id, name: t.name.textContent, ...mediaDiag(t.m, t.stream) })),
    }),
  });
})();
