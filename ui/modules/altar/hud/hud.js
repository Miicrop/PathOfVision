// In-game altar HUD: renders the `altar:hud` view of the scanner thread and sizes the window
// to it (`hud_fit`, physical pixels). All text goes in via `textContent`.
(function () {
  "use strict";

  const tauri = window.__TAURI__;
  const IMG = "../img/";
  const TINT_ALPHA = 60 / 255;
  const NBSP2 = "  ";

  let shown = null;   // JSON of the view on screen

  function el(tag, className, text) {
    const e = document.createElement(tag);
    if (className) e.className = className;
    if (text !== undefined) e.textContent = text;
    return e;
  }

  function img(src) {
    const i = el("img");
    i.alt = "";
    i.addEventListener("error", () => { i.hidden = true; });
    i.src = IMG + src;
    return i;
  }

  function rgba(hex, alpha) {
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
    return `rgba(${r}, ${g}, ${b}, ${alpha.toFixed(3)})`;
  }

  /** Sizes that follow the font size, in CSS px so they come out as `font_size * scale`
   *  physical px: `k` also scales the fixed paddings and borders of hud.css. */
  function applyStyle(style) {
    const k = (style.scale || 1) / window.devicePixelRatio;
    const fs = style.font_size * k;
    const target = Math.round(style.font_size * 3.6) * k;
    const s = document.body.style;
    s.setProperty("--k", String(k));
    s.setProperty("--fs", `${fs}px`);
    s.setProperty("--mod-icon", `${Math.round(style.font_size * 1.6) * k}px`);
    s.setProperty("--target-icon", `${target}px`);
    s.setProperty("--target-col", `${Math.floor(target / k * 2.1) * k}px`);
    s.setProperty("--legend-top", `${Math.max(Math.round(style.font_size * 0.6), 12) * k}px`);
    s.setProperty("--other-opacity", String(style.other_opacity));
  }

  function modRow(grid, row, mod, style) {
    const at = (e, column) => {
      e.style.gridRow = String(row);
      if (column) e.style.gridColumn = String(column);
      grid.append(e);
      return e;
    };
    if (style.tiers && mod.tint) {
      // First, so the row's cells are drawn on top of it.
      at(el("div", "tint")).style.background = rgba(mod.tint, TINT_ALPHA);
    }
    at(el("span", mod.upside ? "sign up" : "sign down", mod.upside ? "+" : "−"), 1);
    const icon = at(el("span", "icon"), 2);
    if (mod.icon) icon.append(img(mod.icon));
    const text = at(el("span", "text"), 3);
    for (const seg of mod.text) {
      const span = el("span", null, seg.text);
      if (seg.color) span.style.color = seg.color;
      text.append(span);
    }
    if (style.values) {
      // Read values in the text color, unread ones as their range in grey.
      const values = at(el("span", "values"), 4);
      mod.values.forEach((v, i) => {
        if (i) values.append(" / ");
        values.append(el("span", v.read ? null : "unread", v.text));
      });
    }
  }

  function optionBox(option, style) {
    const wrap = el("div", "option");
    wrap.style.setProperty("--c", option.color);
    if (option.dimmed && style.other_opacity < 1) wrap.classList.add("dimmed");

    const box = el("div", option.chosen ? "box chosen" : "box");
    const target = el("div", "target");
    if (option.target_icon) target.append(img(option.target_icon));
    box.append(target);

    const grid = el("div", style.values ? "mods" : "mods no-values");
    option.mods.forEach((mod, i) => modRow(grid, i + 1, mod, style));
    box.append(grid);

    wrap.append(box, el("span", "legend", option.verdict));
    return wrap;
  }

  function recommendationBox(altar, style) {
    const rec = altar.recommendation;
    const box = el("div", "rec");
    box.style.setProperty("--c", rec.color);
    const line = el("div", "line");
    if (rec.arrow) {
      line.append(el("span", "arrow", rec.arrow), NBSP2 + rec.text + NBSP2, el("span", "arrow", rec.arrow));
    } else {
      line.textContent = rec.text;
    }
    box.append(line);
    if (!altar.complete) box.append(el("div", "note", "partly hidden, verdict may change"));
    if (style.untested) box.append(el("div", "note", "untested resolution"));
    return box;
  }

  function altarColumn(altar, style) {
    const column = el("div", "column");
    const option = (pos) => altar.options.find((o) => o.position === pos);
    for (const part of [option("top"), null, option("bottom")]) {
      if (part === null) column.append(recommendationBox(altar, style));
      else if (part) column.append(optionBox(part, style));
    }
    return column;
  }

  function build(view) {
    document.body.replaceChildren();
    if (!view.altars.length) return;
    applyStyle(view.style);
    for (const altar of view.altars) document.body.append(altarColumn(altar, view.style));
  }

  /** Sizes and places the window; also on an unchanged view, so it follows the PoE window,
   *  the HUD offsets and DPI changes. */
  function fit(view) {
    let width = 0;
    let height = 0;
    if (view.altars.length) {
      const r = document.body.getBoundingClientRect();
      width = Math.ceil(r.width * window.devicePixelRatio);
      height = Math.ceil(r.height * window.devicePixelRatio);
    }
    tauri.core.invoke("hud_fit", { width, height }).catch((e) => console.error("hud_fit", e));
  }

  function render(view) {
    if (!view || !Array.isArray(view.altars)) view = { altars: [] };
    const json = JSON.stringify(view);
    if (json !== shown) {
      shown = json;
      build(view);
    }
    fit(view);
  }

  if (!tauri) {
    console.error("not running inside Path of Vision");
    return;
  }
  tauri.event.listen("altar:hud", (e) => render(e.payload));
})();
