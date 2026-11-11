// cooee's controls window: your own camera preview, mirrored, beside Mute / Camera / Leave. Knows
// nothing about Meet: it renders the adapter's model (window.__cooee) and acts through
// `__cooee.act`, in the popup popup.js opens.
(function () {
  if (window.top !== window || location.host !== "meet.google.com") return;

  const { media, release, diag: mediaDiag } = window.__cooeeMedia;

  const CSS = `
    #root { display:flex; gap:6px; padding:4px; }
    #preview { position:relative; flex:none; height:100%; aspect-ratio:16/9; background:#000; border-radius:6px; overflow:hidden; }
    #preview video, #preview canvas { width:100%; height:100%; object-fit:cover; display:block; transform:scaleX(-1); }
    #preview .off { position:absolute; inset:0; display:flex; align-items:center; justify-content:center; color:var(--muted); font-size:12px; }
    #buttons { flex:1; display:flex; flex-direction:column; gap:4px; min-width:0; }
    button { all:unset; flex:1; display:flex; align-items:center; justify-content:center; border-radius:6px; background:var(--chip); color:var(--fg); font-size:12px; cursor:pointer; }
    button:hover:not(:disabled) { background:#3a4045; }
    button.off { background:#5a1f1b; color:#ffd9d5; }
    button.leave { background:var(--danger); color:#fff; }
    button:disabled { opacity:.45; cursor:default; }
  `;

  // The preview's current media and the stream it plays.
  let shown = null; // { stream, m }

  function dropPreview() {
    if (shown) release(shown.m);
    shown = null;
  }

  function build(d) {
    const root = d.createElement("div");
    const preview = Object.assign(d.createElement("div"), { id: "preview" });
    const off = Object.assign(d.createElement("div"), { className: "off", textContent: "Camera off" });
    preview.append(off);
    const buttons = Object.assign(d.createElement("div"), { id: "buttons" });
    const button = (kind) => {
      const b = d.createElement("button");
      b.onclick = () => window.__cooee.act(kind);
      buttons.append(b);
      return b;
    };
    const mic = button("mic");
    const camera = button("camera");
    const leave = button("leave");
    leave.className = "leave";
    leave.textContent = "Leave";
    root.append(preview, buttons);
    return { root, preview, off, mic, camera };
  }

  // A device button shows what pressing it does; off reads red; missing from Meet is disabled.
  function device(b, on, whenOn, whenOff, what) {
    b.disabled = on === null;
    b.textContent = on === false ? whenOff : whenOn;
    b.classList.toggle("off", on === false);
    b.title = on === null ? `Meet's ${what} button isn't on the page` : "";
  }

  function render(ui, model) {
    const { d, preview, off, mic, camera } = ui;
    const { micOn, cameraOn, stream } = model.self;
    device(mic, micOn, "Mute", "Unmute", "microphone");
    device(camera, cameraOn, "Stop Camera", "Start Camera", "camera");

    if (shown?.stream !== stream) {
      dropPreview();
      if (stream) {
        shown = { stream, m: media(d, stream) };
        preview.prepend(shown.m.media);
      }
    }
    off.style.display = stream ? "none" : "";
  }

  window.__cooeePopup("controls", {
    size: [340, 100],
    css: CSS,
    build,
    render,
    wanted: (model) => model.inCall,
    reset: dropPreview,
    diag: () => ({ preview: shown && mediaDiag(shown.m, shown.stream) }),
  });
})();
