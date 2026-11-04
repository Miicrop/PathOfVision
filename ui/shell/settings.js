// Settings window: the forms are built only from `settings_schema`; values come from
// `settings_get` and every `settings:changed` event. "general" lives under `general`,
// a module section under `modules.<id>`.
(function () {
  "use strict";

  const $ = (id) => document.getElementById(id);
  const SAVE_DELAY = 150; // sliders fire many events: write once they settle
  const SAVED_FOR = 1500;

  let schema = [];
  let settings = null;
  let current = null; // id of the shown section
  let pending = {}; // "section\u0000key" -> value not written yet
  let renderers = []; // functions that show the current values in the built form

  function sectionValues(id, from) {
    const s = from || settings || {};
    return (id === "general" ? s.general : s.modules && s.modules[id]) || {};
  }

  function valueOf(section, key) {
    const k = section + "\u0000" + key;
    return k in pending ? pending[k] : sectionValues(section)[key];
  }

  // The schema carries no defaults, so an unset value falls back to a neutral one per kind.
  function fallbackOf(field) {
    const k = field.kind;
    switch (k.type) {
      case "toggle":
        return false;
      case "choice":
        return k.options.length ? k.options[0][0] : "";
      case "slider":
      case "number":
        return k.min;
      default:
        return null;
    }
  }

  function shownValue(section, field) {
    return valueOf(section, field.key) ?? fallbackOf(field);
  }

  function showError(e) {
    $("error").textContent = e ? String(e) : "";
    $("error").hidden = !e;
  }

  function showSaved() {
    $("saved").textContent = "Saved";
    clearTimeout(showSaved.timer);
    showSaved.timer = setTimeout(() => ($("saved").textContent = ""), SAVED_FOR);
  }

  function render() {
    renderers.forEach((r) => r());
  }

  function applied(result) {
    settings = result;
    render();
  }

  function change(section, key, value) {
    pending[section + "\u0000" + key] = value;
    render();
    clearTimeout(change.timer);
    change.timer = setTimeout(flush, SAVE_DELAY);
  }

  // Writes everything pending now. All IPC calls are issued synchronously (so this also works
  // from `pagehide`); only the results are awaited.
  async function flush() {
    clearTimeout(change.timer);
    const writes = Object.entries(pending).filter(([, value]) => value !== undefined);
    pending = {};
    if (!writes.length) return;
    let ok = true;
    const calls = writes.map(([k, value]) => {
      const [section, key] = k.split("\u0000");
      return window.pov.invoke("settings_set", { section, key, value }).then(
        (result) => {
          settings = result;
        },
        (e) => {
          ok = false;
          showError(e);
        }
      );
    });
    await Promise.all(calls);
    if (ok) {
      showError(null);
      showSaved();
    }
    render();
  }

  async function reset(section, keys) {
    keys.forEach((key) => delete pending[section + "\u0000" + key]);
    try {
      applied(await window.pov.invoke("settings_reset", { section, keys }));
      showError(null);
      showSaved();
    } catch (e) {
      showError(e);
      render();
    }
  }

  function el(tag, props, children) {
    const e = document.createElement(tag);
    Object.assign(e, props || {});
    (children || []).forEach((c) => e.append(c));
    return e;
  }

  // Segmented buttons: `options` are [value, label] pairs, values compared as strings.
  function segmented(section, field, options) {
    const seg = el("div", { className: "seg" });
    seg.setAttribute("role", "group");
    seg.setAttribute("aria-label", field.label);
    const buttons = options.map(([value, label]) => {
      const b = el("button", { type: "button", textContent: label });
      b.addEventListener("click", () => change(section, field.key, value));
      seg.append(b);
      return [value, b];
    });
    renderers.push(() => {
      const v = String(shownValue(section, field));
      buttons.forEach(([value, b]) => b.setAttribute("aria-pressed", String(String(value) === v)));
    });
    return seg;
  }

  function slider(section, field, kind) {
    const scale = kind.scale || 1;
    const shown = (v) => Math.round(v * scale * 1000) / 1000;
    const input = el("input", { type: "range", min: shown(kind.min), max: shown(kind.max), step: shown(kind.step) });
    input.setAttribute("aria-label", field.label);
    const out = el("output");
    input.addEventListener("input", () => change(section, field.key, Number(input.value) / scale));
    renderers.push(() => {
      const v = shown(Number(shownValue(section, field)));
      input.value = v;
      out.textContent = kind.unit ? `${v} ${kind.unit}` : String(v);
    });
    return el("div", { className: "ctl" }, [input, out]);
  }

  function number(section, field, kind) {
    const input = el("input", { type: "number", min: kind.min, max: kind.max, step: kind.step });
    input.setAttribute("aria-label", field.label);
    input.addEventListener("change", () => {
      if (input.value.trim() === "" || !Number.isFinite(Number(input.value))) return render();
      change(section, field.key, Number(input.value));
      flush();
    });
    input.addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      if (input.value.trim() === "" || !Number.isFinite(Number(input.value))) return render();
      change(section, field.key, Number(input.value));
      flush();
    });
    renderers.push(() => {
      input.value = shownValue(section, field);
    });
    const parts = [input];
    if (kind.unit) parts.push(document.createTextNode(kind.unit));
    return el("div", { className: "ctl" }, parts);
  }

  function path(section, field, kind) {
    const input = el("input", { type: "text", placeholder: kind.placeholder || "", spellcheck: false });
    input.setAttribute("aria-label", field.label);
    input.addEventListener("change", () => {
      const v = input.value.trim();
      change(section, field.key, v === "" ? null : v);
      flush();
    });
    input.addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      const v = input.value.trim();
      change(section, field.key, v === "" ? null : v);
      flush();
    });
    renderers.push(() => {
      input.value = shownValue(section, field) ?? "";
    });
    return el("div", { className: "ctl" }, [input]);
  }

  function control(section, field) {
    const kind = field.kind;
    switch (kind.type) {
      case "toggle":
        return segmented(section, field, [[true, "On"], [false, "Off"]]);
      case "choice":
        return segmented(section, field, kind.options);
      case "slider":
        return slider(section, field, kind);
      case "number":
        return number(section, field, kind);
      case "path":
        return path(section, field, kind);
      default:
        return el("span", { className: "muted", textContent: `unsupported field type ${kind.type}` });
    }
  }

  function buildGroup(section, group) {
    const box = el("section", { className: "group" });
    box.setAttribute("aria-label", group.name);
    box.append(el("h2", { className: "grp", textContent: group.name }));
    for (const field of group.fields) {
      box.append(el("span", { className: "lbl", textContent: field.label }));
      const cell = el("div", { className: "field" }, [control(section, field)]);
      if (field.help) cell.append(el("small", { textContent: field.help }));
      box.append(cell);
    }
    if (group.reset) {
      const keys = group.fields.map((f) => f.key);
      const button = el("button", { type: "button", className: "btn", textContent: "Reset to defaults" });
      button.addEventListener("click", () => reset(section, keys));
      box.append(el("span"), el("div", { className: "ctl" }, [button]));
    }
    return box;
  }

  function show(id) {
    const section = schema.find((s) => s.id === id) || schema[0];
    if (!section) return;
    current = section.id;
    document.querySelectorAll("#sections button").forEach((b) =>
      b.setAttribute("aria-current", String(b.dataset.id === current))
    );
    $("title").textContent = section.name;
    renderers = [];
    $("groups").replaceChildren(...section.groups.map((g) => buildGroup(section.id, g)));
    history.replaceState(null, "", `?section=${encodeURIComponent(current)}`);
    if (settings) render();
  }

  function buildSections() {
    $("sections").replaceChildren(
      ...schema.map((s) => {
        const b = el("button", { type: "button", textContent: s.name });
        b.dataset.id = s.id;
        b.addEventListener("click", () => show(s.id));
        return b;
      })
    );
  }

  (async () => {
    try {
      schema = await window.pov.invoke("settings_schema");
      buildSections();
      show(new URLSearchParams(location.search).get("section"));
      applied(await window.pov.invoke("settings_get"));
    } catch (e) {
      showError(e);
    }
  })();
  window.addEventListener("pagehide", () => {
    flush();
  });
  window.pov.listen("settings:changed", (e) => applied(e.payload));
  window.pov.listen("settings:section", (e) => show(e.payload));
})();
