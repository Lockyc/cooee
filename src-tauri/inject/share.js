// cooee's share window: another participant's screen share, whole. Knows nothing about Meet: it
// renders the adapter's model (window.__cooee) into the popup popup.js opens. Rust destroys the
// window when the share ends.
(function () {
  if (window.top !== window || location.host !== "meet.google.com") return;

  const { media, release, diag: mediaDiag } = window.__cooeeMedia;

  const CSS = `
    #root { background:#000; }
    #root video, #root canvas { width:100%; height:100%; object-fit:contain; display:block; }
  `;

  let shown = null; // { stream, m }

  function drop() {
    if (shown) release(shown.m);
    shown = null;
  }

  function render(ui, model) {
    const p = model.presentation;
    if (shown?.stream === p?.stream) return;
    drop();
    if (p) {
      shown = { stream: p.stream, m: media(ui.d, p.stream) };
      ui.root.append(shown.m.media);
    }
  }

  window.__cooeePopup("share", {
    css: CSS,
    build: (d) => ({ root: d.createElement("div") }),
    render,
    wanted: (model) => model.inCall && !!model.presentation,
    reset: drop,
    diag: () => ({ share: shown && mediaDiag(shown.m, shown.stream) }),
  });
})();
