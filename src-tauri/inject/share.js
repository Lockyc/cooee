// cooee's share window: another participant's screen share, whole, under a toolbar that sizes the
// window and minimises it to a thumbnail. Knows nothing about Meet: it renders the adapter's model
// (window.__cooee) into the popup popup.js opens. Rust sizes and places the window
// (`share_window`) and destroys it when the share ends.
(function () {
  if (window.top !== window || location.host !== "meet.google.com") return;

  const { media, release, diag: mediaDiag } = window.__cooeeMedia;

  const CSS = `
    #root { display:flex; flex-direction:column; background:#000; }
    #bar { flex:none; display:flex; gap:4px; padding:4px; background:var(--bg); }
    #bar button { all:unset; flex:1; min-width:0; height:20px; display:flex; align-items:center; justify-content:center; border-radius:5px; background:var(--chip); color:var(--fg); font-size:11px; cursor:pointer; white-space:nowrap; overflow:hidden; }
    #bar button:hover { background:#3a4045; }
    #bar button.minimise { flex:1.4; }
    #screen { flex:1; min-height:0; }
    #screen.minimised { cursor:pointer; }
    #screen video, #screen canvas { width:100%; height:100%; object-fit:contain; display:block; }
  `;

  const SIZES = [
    ["small", "Small"],
    ["medium", "Medium"],
    ["large", "Large"],
    ["fill", "Fill"],
  ];

  let shown = null; // { stream, m }

  function drop() {
    if (shown) release(shown.m);
    shown = null;
  }

  function build(d) {
    const root = d.createElement("div");
    const bar = Object.assign(d.createElement("div"), { id: "bar" });
    const screen = Object.assign(d.createElement("div"), { id: "screen" });
    const toggle = Object.assign(d.createElement("button"), { className: "minimise" });
    const ui = { root, screen, toggle, minimised: false };

    const showMinimised = (minimised) => {
      ui.minimised = minimised;
      toggle.textContent = minimised ? "Restore" : "Minimise";
      screen.classList.toggle("minimised", minimised);
      screen.title = minimised ? "Restore" : "";
    };
    const act = (action) =>
      window.__TAURI_INTERNALS__
        ?.invoke("share_window", { action })
        .then(showMinimised)
        .catch((e) => console.warn("cooee: share window", action, e));

    for (const [action, label] of SIZES) {
      const b = Object.assign(d.createElement("button"), { textContent: label });
      b.onclick = () => act(action);
      bar.append(b);
    }
    toggle.onclick = () => act(ui.minimised ? "restore" : "minimise");
    screen.onclick = () => ui.minimised && act("restore");
    bar.append(toggle);
    showMinimised(false);

    root.append(bar, screen);
    return ui;
  }

  function render(ui, model) {
    const p = model.presentation;
    if (shown?.stream === p?.stream) return;
    drop();
    if (p) {
      shown = { stream: p.stream, m: media(ui.d, p.stream) };
      ui.screen.append(shown.m.media);
    }
  }

  window.__cooeePopup("share", {
    css: CSS,
    build,
    render,
    wanted: (model) => model.inCall && !!model.presentation,
    reset: drop,
    diag: () => ({ share: shown && mediaDiag(shown.m, shown.stream) }),
  });
})();
