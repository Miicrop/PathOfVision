"use strict";

// Backend adapter: the app's IPC commands (window.pov from ui/shell/shell.js).
// Commands fail with a message; saving and deleting report it as { ok: false, error } like before.
function connectBackend() {
  if (!window.pov || !window.__TAURI__) {
    return Promise.reject(new Error("No backend. Open the board in Path of Vision (sidebar: Altars)."));
  }
  const call = (cmd, args) => window.pov.invoke(cmd, args);
  const failed = e => ({ ok: false, error: String(e) });
  return Promise.resolve({
    getState: () => call("altar_get_state"),
    loadProfile: file => call("altar_load_profile", { file }),
    saveProfile: (file, profile) => call("altar_save_profile", { file, profile }).catch(failed),
    duplicateProfile: (file, name) => call("altar_duplicate_profile", { file, name }),
    deleteProfile: file => call("altar_delete_profile", { file }).catch(failed),
    setActive: file => call("altar_set_active", { file }),
    exportCode: file => call("altar_export_code", { file }),
    importPreview: code => call("altar_import_preview", { code }),
    importCode: code => call("altar_import_code", { code }),
  });
}

const ALTAR = { eater: ["Eater", "var(--eater)"], exarch: ["Exarch", "var(--exarch)"] };
const TARGET = { player: "Player", minions: "Minions", boss: "Map Boss" };
const ICON_DIR = "../img/mods/";   // reward icons named in data/labels.json
const DOWN_COLS = [
  { key: null, name: "Fine", tier: "var(--t-fine)", hint: "Unrated downsides count as fine" },
  { key: "dangerous", name: "Dangerous", tier: "var(--t-danger)", hint: "Gives a warning" },
  { key: "brick", name: "Brick", tier: "var(--t-brick)", hint: "Never take, unless the upside is in the top column or a key column (★)" },
];

let api, state, byId, profile, file;
let dragging = null;                                   // ids of the cards being dragged
const dropMark = document.createElement("div");         // line where the card will land
dropMark.className = "drop-mark";
const filter = { altar: "both", side: "all", target: "all", reward: "all", q: "" };
const $ = id => document.getElementById(id);

const text = m => m.lines.join(" / ");

// HTML of a downside text with its keywords (data/keywords.json, whole words, any case) colored.
let keywordRe = null;
const escapeHtml = s => s.replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
function colorKeywords(s) {
  const kw = state.keywords || {};
  if (!keywordRe && Object.keys(kw).length) {
    const words = Object.keys(kw).sort((a, b) => b.length - a.length).map(k => k.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    keywordRe = new RegExp(`\\b(${words.join("|")})\\b`, "gi");
  }
  const colors = Object.fromEntries(Object.entries(kw).map(([k, c]) => [k.toLowerCase(), c]));
  const out = escapeHtml(s);
  return keywordRe ? out.replace(keywordRe, w => `<span style="color:${colors[w.toLowerCase()]}">${w}</span>`) : out;
}

// Reward group of an upside, from its icon name in labels.json.
function rewardOf(m) {
  const icon = m.icon || "";
  if (icon.endsWith("-scarab")) return "scarab";
  if (icon.startsWith("div-card-")) return "divcard";
  if (icon.startsWith("dup-")) return "dup";
  if (icon.startsWith("stat-")) return "stat";
  if (icon.includes("eldritch-")) return "eldritch";
  return icon ? "currency" : "";
}

function visible(m) {
  if (filter.altar !== "both" && !m.altars.includes(filter.altar)) return false;
  if (filter.side !== "all" && m.side !== filter.side) return false;
  if (filter.target !== "all" && m.target !== filter.target) return false;
  if (filter.reward !== "all" && rewardOf(m) !== filter.reward) return false;
  if (filter.q && !`${text(m)} ${m.alias || ""}`.toLowerCase().includes(filter.q)) return false;   // short label too, e.g. "Turbo"
  return true;
}

// Columns left to right. `list` is the profile array behind a column (null = everything not listed).
// Upsides: Unrated | the user's own columns | top column (the only one that can outweigh a Brick).
function columnsFor(side) {
  if (side === "downside") return DOWN_COLS.map(c => ({ ...c, list: c.key ? (profile.downsides[c.key] ||= []) : null }));
  const up = profile.upsides;
  return [
    { kind: "unrated", name: "Unrated", tier: "var(--t-un)", list: null },
    // A key column ("key": true) is treated like the top column: it can outweigh a Brick, gold in the HUD.
    ...up.columns.map((c, index) => ({ kind: "col", index, name: c.name, key: !!c.key, list: c.ids,
                                       tier: c.key ? "var(--t-s)" : "var(--t-a)",
                                       title: c.key ? "Key column: treated like the top column, can outweigh a Brick downside" : "" })),
    { kind: "top", name: up.top.name, tier: "var(--t-s)", list: up.top.ids,
      title: "Top column: the only one that can make a Brick downside worth it (ADAPT - BRICK)" },
  ];
}

const allLists = side => columnsFor(side).map(c => c.list).filter(Boolean);

// Ids in a column, in priority order.
function columnIds(side, col) {
  if (col.list) return col.list.filter(id => byId[id]);
  const listed = new Set(allLists(side).flat());
  return state.mods.filter(m => m.side === side && !listed.has(m.id)).map(m => m.id);
}

// Undo / redo (Ctrl+Z, Ctrl+Y or Ctrl+Shift+Z): snapshots of the tiers before each change.
const undoStack = [], redoStack = [];
const snapshot = () => JSON.stringify({ upsides: profile.upsides, downsides: profile.downsides });
function remember() {
  undoStack.push(snapshot());
  if (undoStack.length > 100) undoStack.shift();
  redoStack.length = 0;
}
function restore(from, to, label) {
  if (profile.readOnly || !from.length) return;
  to.push(snapshot());
  Object.assign(profile, JSON.parse(from.pop()));
  render();
  save(label);
}
const undo = () => restore(undoStack, redoStack, "Undone");
const redo = () => restore(redoStack, undoStack, "Redone");

// Moves the cards (kept in the given order) into a column, in front of beforeId or at the end.
function moveCards(ids, side, colIndex, beforeId) {
  if (profile.readOnly || !ids.length) return;
  remember();
  for (const list of allLists(side)) {
    for (const id of ids) {
      const at = list.indexOf(id);
      if (at >= 0) list.splice(at, 1);
    }
  }
  const target = columnsFor(side)[colIndex].list;
  if (target) {
    const at = beforeId ? target.indexOf(beforeId) : -1;
    if (at >= 0) target.splice(at, 0, ...ids); else target.push(...ids);
  }
  render();
  save();
}

// Selection: click selects one card, Ctrl+click adds or removes one, Shift+click selects the
// range from the last clicked card within its column. Only one side at a time.
const selected = new Set();
let anchor = null;                                     // last clicked card, start of a Shift range

function showSelection() {
  document.querySelectorAll(".card").forEach(el => el.classList.toggle("selected", selected.has(el.dataset.id)));
}

function clickCard(e, el) {
  const id = el.dataset.id, side = byId[id].side;
  if ([...selected].some(s => byId[s].side !== side)) selected.clear();
  const list = el.closest(".list");
  const ids = [...list.querySelectorAll(".card")].map(c => c.dataset.id);
  if (e.shiftKey && anchor && ids.includes(anchor)) {
    const [a, b] = [ids.indexOf(anchor), ids.indexOf(id)].sort((x, y) => x - y);
    if (!e.ctrlKey) selected.clear();
    ids.slice(a, b + 1).forEach(s => selected.add(s));
  } else if (e.ctrlKey || e.metaKey) {
    selected.has(id) ? selected.delete(id) : selected.add(id);
    anchor = id;
  } else {
    const only = selected.size === 1 && selected.has(id);
    selected.clear();
    if (!only) selected.add(id);                      // clicking the only selected card deselects it
    anchor = id;
  }
  showSelection();
}

// Selected ids in board order: column by column, top to bottom.
function selectedInOrder(side) {
  return columnsFor(side).flatMap(col => columnIds(side, col)).filter(id => selected.has(id));
}

// ← / → move the selection one column; every card steps on from where it is.
function stepSelection(step) {
  const ids = [...selected];
  if (profile.readOnly || !ids.length) return;
  const side = byId[ids[0]].side;
  const cols = columnsFor(side);
  const colOf = id => cols.findIndex(col => columnIds(side, col).includes(id));
  const order = selectedInOrder(side);
  // Move the column nearest the edge first, so cards from neighboring columns keep their order.
  const groups = [...new Set(order.map(colOf))].sort((a, b) => step > 0 ? b - a : a - b);
  if (!groups.some(from => from + step >= 0 && from + step < cols.length)) return;
  remember();
  for (const from of groups) {
    const to = from + step;
    if (to < 0 || to >= cols.length) continue;
    const ids = order.filter(id => colOf(id) === from);
    for (const list of allLists(side)) for (const id of ids) { const at = list.indexOf(id); if (at >= 0) list.splice(at, 1); }
    if (cols[to].list) cols[to].list.push(...ids);
  }
  render();
  save();
}

/**
 * The app's own question dialog instead of prompt()/confirm(). With `value` it asks for a name and
 * resolves to the trimmed text (null on cancel or empty); without, it confirms and resolves to true/false.
 */
function ask({ title, text, value, ok = "OK", danger = false }) {
  const dialog = $("ask-dialog"), input = $("ask-input"), okBtn = $("ask-ok");
  const naming = value !== undefined;
  $("ask-h").textContent = title;
  $("ask-text").textContent = text;
  input.hidden = !naming;
  input.value = naming ? value : "";
  okBtn.textContent = ok;
  okBtn.classList.toggle("danger", danger);
  const sync = () => (okBtn.disabled = naming && !input.value.trim());
  input.oninput = sync;
  sync();
  return new Promise(resolve => {
    let answer = naming ? null : false;
    $("ask-cancel").onclick = () => dialog.close();
    $("ask-form").onsubmit = e => {
      e.preventDefault();
      if (okBtn.disabled) return;
      answer = naming ? input.value.trim() : true;
      dialog.close();
    };
    dialog.onclose = () => resolve(answer);
    dialog.showModal();
    if (naming) { input.focus(); input.select(); } else okBtn.focus();
  });
}

// Column management (upsides only): add, rename, move, delete.
function editColumns(change) {
  if (profile.readOnly) return;
  remember();
  change(profile.upsides);
  render();
  save();
}
async function addColumn() {
  const name = await ask({ title: "New column", text: "Name of the new column", value: "New column", ok: "Add" });
  if (name) editColumns(up => up.columns.push({ name, ids: [] }));
}
async function renameColumn(col) {
  const name = await ask({ title: "Rename column", text: "Column name", value: col.name, ok: "Rename" });
  if (!name) return;
  editColumns(up => { (col.kind === "top" ? up.top : up.columns[col.index]).name = name; });
}
function toggleKey(col) {
  editColumns(up => {
    const c = up.columns[col.index];
    if (c.key) delete c.key; else c.key = true;
  });
}
function moveColumn(col, step) {
  editColumns(up => {
    const to = col.index + step;
    if (to < 0 || to >= up.columns.length) return;
    [up.columns[col.index], up.columns[to]] = [up.columns[to], up.columns[col.index]];
  });
}
async function deleteColumn(col) {
  const count = profile.upsides.columns[col.index].ids.length;
  if (count && !(await ask({ title: "Delete column", text: `Delete "${col.name}"? Its ${count} cards go back to Unrated.`, ok: "Delete", danger: true }))) return;
  editColumns(up => up.columns.splice(col.index, 1));
}

async function save(label = "Saved") {
  const result = await api.saveProfile(file, { ...profile, modsVersion: state.gameVersion });
  $("saved").textContent = result.ok ? label : `Not saved: ${result.error}`;
  clearTimeout(save.timer);
  save.timer = setTimeout(() => ($("saved").textContent = ""), 1500);
}

function cardEl(m) {
  const el = document.createElement("div");
  el.className = `card ${m.side}` + (profile.readOnly ? " locked" : "") + (selected.has(m.id) ? " selected" : "");
  el.dataset.id = m.id;
  el.draggable = !profile.readOnly;
  // The frame color shows the altar (split diagonally when the mod rolls on both), the faded
  // scene behind the text shows the target, the icon on the right shows the reward.
  el.dataset.alt = m.altars.length > 1 ? "both" : m.altars[0];
  el.innerHTML = `
    ${m.altars.map(a => `<span class="art art-${a}" style="background-image:url(img/art_${m.target}_${a}.png)"></span>`).join("")}
    ${m.altars.map(a => `<span class="fr fr-${a}"></span>`).join("")}
    <div class="body">
      <span class="sr">${m.altars.map(a => ALTAR[a][0]).join(", ")}</span>
      <div class="meta"><span class="chip">${TARGET[m.target]}</span></div>
      <div class="head"></div>
      <div class="txt"></div>
    </div>
    ${m.icon ? `<img class="reward" src="${ICON_DIR}${m.icon}.png" alt="">` : ""}`;
  el.classList.toggle("has-reward", !!m.icon);
  // Both texts are always there; the card style (body[data-cards]) decides what shows.
  const full = text(m), alias = m.alias || full;
  const paint = (sel, s) => { const n = el.querySelector(sel); if (m.side === "downside") n.innerHTML = colorKeywords(s); else n.textContent = s; };
  paint(".head", alias);
  paint(".txt", full);
  el.querySelector(".head").title = full;   // the compact style hides the mod text
  if (!profile.readOnly) el.addEventListener("click", e => clickCard(e, el));
  el.addEventListener("dragstart", e => {
    // Dragging a selected card takes the whole selection along; any other card goes alone.
    dragging = selected.has(m.id) ? selectedInOrder(m.side) : [m.id];
    e.dataTransfer.setData("text/plain", dragging.join(" "));
    e.dataTransfer.effectAllowed = "move";
    const els = dragging.map(id => document.querySelector(`.card[data-id="${id}"]`)).filter(Boolean);
    requestAnimationFrame(() => els.forEach(c => c.classList.add("dragging")));   // after the browser took the drag image
  });
  el.addEventListener("dragend", () => {
    dragging = null;
    document.querySelectorAll(".card.dragging").forEach(c => c.classList.remove("dragging"));
    dropMark.remove();
    document.querySelectorAll(".col.over").forEach(c => c.classList.remove("over"));
  });
  return el;
}

function headerEl(col, count, colCount) {
  const header = document.createElement("header");
  const name = document.createElement("b");
  name.textContent = col.name;
  if (col.key) name.insertAdjacentHTML("afterbegin", '<span class="star" aria-label="Key column">★ </span>');
  if (col.title) name.title = col.title;
  header.appendChild(name);
  const tools = document.createElement("span");
  tools.className = "tools";
  const tool = (label, title, action, disabled = false) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = label;
    b.title = title;
    b.setAttribute("aria-label", `${title}: ${col.name}`);
    b.disabled = disabled;
    b.addEventListener("click", action);
    tools.appendChild(b);
  };
  if (!profile.readOnly && col.kind === "col") {
    const last = profile.upsides.columns.length - 1;
    tool("‹", "Move column left", () => moveColumn(col, -1), col.index === 0);
    tool("›", "Move column right", () => moveColumn(col, 1), col.index === last);
  }
  if (!profile.readOnly && col.kind === "col") {
    tool(col.key ? "★" : "☆", col.key ? "Unmark as key column" : "Mark as key column (treated like the top column)", () => toggleKey(col));
  }
  if (!profile.readOnly && (col.kind === "col" || col.kind === "top")) tool("✎", "Rename column", () => renameColumn(col));
  if (!profile.readOnly && col.kind === "col") tool("×", "Delete column", () => deleteColumn(col));
  header.appendChild(tools);
  const counter = document.createElement("span");
  counter.className = "count";
  counter.textContent = count;
  header.appendChild(counter);
  if (col.kind === "top") {
    // New columns appear left of the top column, so the button sits on it (always visible).
    const add = document.createElement("button");
    add.type = "button";
    add.className = "add-col";
    add.innerHTML = '<span class="plus" aria-hidden="true">+</span>Column';
    add.disabled = profile.readOnly;
    add.title = profile.readOnly ? "Read-only profile: duplicate it to add columns" : "Add a column left of this one";
    add.addEventListener("click", addColumn);
    header.appendChild(add);
  }
  return header;
}

function renderBoard(side) {
  const wrap = $("cols-" + side);
  const scrolled = [...wrap.querySelectorAll(".list")].map(l => l.scrollTop);   // keep each column where it was
  wrap.innerHTML = "";
  const cols = columnsFor(side);
  // Columns keep a readable width; wide screens show more of them instead of wider cards.
  wrap.style.gridTemplateColumns = `repeat(${cols.length}, minmax(240px, 340px))`;
  cols.forEach((col, i) => {
    const ids = columnIds(side, col).filter(id => visible(byId[id]));
    const colEl = document.createElement("div");
    colEl.className = "col";
    colEl.style.setProperty("--tier", col.tier);
    colEl.appendChild(headerEl(col, ids.length, cols.length));
    if (col.hint) colEl.insertAdjacentHTML("beforeend", `<div class="hint">${col.hint}</div>`);
    const list = document.createElement("div");
    list.className = "list";
    if (!ids.length) list.innerHTML = `<div class="empty">${profile.readOnly ? "No cards" : "Drop cards here"}</div>`;
    ids.forEach(id => list.appendChild(cardEl(byId[id])));
    // The whole column is the drop target; over its header the card goes to the top.
    colEl.addEventListener("dragover", e => {
      if (profile.readOnly || !dragging || byId[dragging[0]].side !== side) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      document.querySelectorAll(".col.over").forEach(c => c !== colEl && c.classList.remove("over"));
      colEl.classList.add("over");
      const cards = [...list.querySelectorAll(".card:not(.dragging)")];
      const before = e.target.closest("header, .hint") ? cards[0]
        : cards.find(el => e.clientY < el.getBoundingClientRect().top + el.offsetHeight / 2);
      if (before) list.insertBefore(dropMark, before); else list.appendChild(dropMark);
      autoScroll(list, e.clientY);
    });
    colEl.addEventListener("dragleave", e => {
      if (colEl.contains(e.relatedTarget)) return;
      colEl.classList.remove("over");
      if (dropMark.parentNode === list) dropMark.remove();
    });
    colEl.addEventListener("drop", e => {
      e.preventDefault();
      const ids = dragging;
      if (!ids || byId[ids[0]].side !== side) return;
      let next = dropMark.nextElementSibling;
      while (next && (!next.classList.contains("card") || next.classList.contains("dragging"))) next = next.nextElementSibling;
      dropMark.remove();
      moveCards(ids, side, i, next ? next.dataset.id : null);
    });
    colEl.appendChild(list);
    wrap.appendChild(colEl);
    list.scrollTop = scrolled[i] || 0;
  });
}

// While dragging near the top or bottom edge of a column or the window, scroll it.
// dragover keeps firing while the pointer rests, so this runs continuously.
function autoScroll(list, y) {
  const r = list.getBoundingClientRect(), edge = 50, step = 18;
  if (y < r.top + edge) list.scrollTop -= step;
  else if (y > r.bottom - edge) list.scrollTop += step;
  if (y < edge) window.scrollBy(0, -step);
  else if (y > window.innerHeight - edge) window.scrollBy(0, step);
}

function renderProfiles() {
  const sel = $("profile");
  sel.innerHTML = "";
  for (const p of state.profiles) sel.add(new Option(p.name + (p.readOnly ? " (read-only)" : ""), p.file));
  sel.value = file;
}

function render() {
  $("board-upside").hidden = filter.side === "downside";
  $("board-downside").hidden = filter.side === "upside" || filter.reward !== "all";   // downsides have no reward
  $("ro").hidden = !profile.readOnly;
  $("del").disabled = profile.readOnly;
  for (const id of selected) if (!visible(byId[id])) selected.delete(id);   // filtered-out cards never move unseen
  renderBoard("upside");
  renderBoard("downside");
}

async function openProfile(name) {
  if (!state.profiles.some(p => p.file === name)) state.profiles = (await api.getState()).profiles;
  file = name;
  profile = await api.loadProfile(file);
  selected.clear();
  undoStack.length = redoStack.length = 0;          // history belongs to one profile
  profile.upsides ||= { columns: [], top: { name: "S", ids: [] } };
  profile.downsides ||= {};
  await api.setActive(file);   // the selected profile is the one the overlay uses
  renderProfiles();
  render();
}

function showError(message) {
  $("err").textContent = message;
  $("err").hidden = false;
}

document.querySelectorAll(".seg[data-filter]").forEach(seg => seg.addEventListener("click", e => {
  const b = e.target.closest("button");
  if (!b) return;
  seg.querySelectorAll("button").forEach(x => x.setAttribute("aria-pressed", String(x === b)));
  filter[seg.dataset.filter] = b.dataset.v;
  render();
}));
document.addEventListener("keydown", e => {
  if (e.target.closest("input, select, textarea")) return;   // the search box keeps its own undo
  if (document.querySelector("dialog[open]")) return;        // keys belong to the open dialog
  const key = e.key.toLowerCase();
  if (e.ctrlKey && !e.altKey && (key === "z" || key === "y")) {
    e.preventDefault();
    key === "y" || e.shiftKey ? redo() : undo();
    return;
  }
  if (!selected.size) return;
  if (e.key === "Escape") { selected.clear(); showSelection(); }
  else if (e.key === "ArrowLeft" || e.key === "ArrowRight") { e.preventDefault(); stepSelection(e.key === "ArrowLeft" ? -1 : 1); }
});
document.addEventListener("click", e => {
  if (selected.size && !e.target.closest(".card, button") && e.target.closest(".board")) { selected.clear(); showSelection(); }
});
$("q").addEventListener("input", e => { filter.q = e.target.value.trim().toLowerCase(); render(); });
$("profile").addEventListener("change", e => openProfile(e.target.value));
$("dup").addEventListener("click", async () => {
  const name = await ask({ title: "Duplicate profile", text: "Name of the new profile", value: `${profile.name} (copy)`, ok: "Duplicate" });
  if (!name) return;
  let result;
  try { result = await api.duplicateProfile(file, name); } catch (e) { return showError(String(e)); }
  state.profiles = result.profiles;
  await openProfile(result.file);
});
$("del").addEventListener("click", async () => {
  if (profile.readOnly) return;
  if (!(await ask({ title: "Delete profile", text: `Delete the profile "${profile.name}"? This cannot be undone; export it first if you want to keep a copy.`, ok: "Delete", danger: true }))) return;
  const result = await api.deleteProfile(file);
  if (!result.ok) return showError(result.error);
  state.profiles = result.profiles;
  await openProfile(result.active);
});

// Export: the profile as a text code on the clipboard; if that fails, a dialog to copy it by hand.
function flash(message) {
  $("saved").textContent = message;
  clearTimeout(save.timer);
  save.timer = setTimeout(() => ($("saved").textContent = ""), 1500);
}
$("export").addEventListener("click", async () => {
  let code;
  try { code = await api.exportCode(file); } catch (e) { return showError(String(e)); }
  try {
    await navigator.clipboard.writeText(code);
    flash("Code copied");
  } catch {
    const box = $("export-code");
    box.value = code;
    $("export-dialog").showModal();
    box.focus();
    box.select();
  }
});

// Import: paste a code, see what it holds, then add it as a new profile.
const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
let previewRun = 0;                                    // only the newest preview request counts
function showPreview(text, kind) {
  const p = $("import-preview");
  p.textContent = text;
  p.className = "preview" + (kind ? " " + kind : "");
}
async function updatePreview() {
  const code = $("import-code").value;
  const run = ++previewRun;
  $("import-ok").disabled = true;
  if (!code.trim()) return showPreview("", "");
  try {
    const p = await api.importPreview(code);
    if (run !== previewRun) return;
    let text = `Profile '${p.name}' – ${plural(p.columns, "column")}, ${p.upsides} rated upside${p.upsides === 1 ? "" : "s"}, ${p.dangerous} Dangerous, ${p.brick} Brick`;
    if (p.unknown > 0) text += ` (${plural(p.unknown, "option")} unknown to this version, skipped)`;
    showPreview(text, "ok");
    $("import-ok").disabled = false;
  } catch (e) {
    if (run === previewRun) showPreview(String(e), "error");
  }
}
$("import").addEventListener("click", () => {
  $("import-code").value = "";
  showPreview("", "");
  $("import-ok").disabled = true;
  $("import-dialog").showModal();
  $("import-code").focus();
});
$("import-code").addEventListener("input", () => {
  clearTimeout(updatePreview.timer);
  updatePreview.timer = setTimeout(updatePreview, 150);
});
$("import-cancel").addEventListener("click", () => $("import-dialog").close());
$("import-form").addEventListener("submit", async e => {
  e.preventDefault();                                  // the dialog stays open if importing fails
  const ok = $("import-ok");
  if (ok.disabled) return;
  ok.disabled = true;
  let result;
  try { result = await api.importCode($("import-code").value); } catch (err) { return showPreview(String(err), "error"); }
  $("import-dialog").close();
  state.profiles = result.profiles;
  await openProfile(result.file);
  flash(result.skipped ? `Imported, ${plural(result.skipped, "option")} skipped` : "Imported");
});

// Card style and target tag come from the settings window and follow its changes live.
function showCardSettings(settings) {
  const altar = (settings && settings.modules && settings.modules.altar) || {};
  if (altar.board_cards !== undefined) document.body.dataset.cards = altar.board_cards;
  if (altar.board_target_tag !== undefined) document.body.dataset.targetTag = String(altar.board_target_tag);
}

(async () => {
  try {
    api = await connectBackend();
    state = await api.getState();
    byId = Object.fromEntries(state.mods.map(m => [m.id, m]));
    $("subtitle").textContent = `${state.mods.length} mods · game ${state.gameVersion} · changes apply to the overlay instantly`;
    window.pov.listen("settings:changed", e => showCardSettings(e.payload));
    showCardSettings(await window.pov.invoke("settings_get"));
    await openProfile(state.active);
  } catch (e) {
    $("subtitle").textContent = "";
    showError(e instanceof Error ? e.message : String(e));   // commands fail with a plain string
  }
})();
