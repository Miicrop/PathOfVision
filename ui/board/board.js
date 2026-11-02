"use strict";

// Backend adapter. Qt WebChannel in the Python prototype; a WebView2 host only needs
// another implementation of these calls.
function connectBackend() {
  return new Promise((resolve, reject) => {
    if (typeof qt === "undefined" || typeof QWebChannel === "undefined") {
      reject(new Error("No backend. Open the board with: python -m prototype.board"));
      return;
    }
    new QWebChannel(qt.webChannelTransport, channel => {
      const b = channel.objects.backend;
      const call = (fn, ...args) => new Promise(done => b[fn](...args, r => done(r === undefined ? null : JSON.parse(r))));
      resolve({
        getState: () => call("getState"),
        loadProfile: file => call("loadProfile", file),
        saveProfile: (file, profile) => call("saveProfile", file, JSON.stringify(profile)),
        duplicateProfile: (file, name) => call("duplicateProfile", file, name),
        deleteProfile: file => call("deleteProfile", file),
        setActive: file => new Promise(done => b.setActive(file, done)),
        getSettings: () => call("getSettings"),
        saveSettings: changes => call("saveSettings", JSON.stringify(changes)),
      });
    });
  });
}

const ALTAR = { eater: ["Eater", "var(--eater)"], exarch: ["Exarch", "var(--exarch)"] };
const TARGET = { player: "Player", minions: "Minions", boss: "Map Boss" };
const COLS = {
  upside: [
    { id: null, name: "Unrated", tier: "var(--t-un)" },
    { id: "b", name: "B", tier: "var(--t-b)" },
    { id: "a", name: "A", tier: "var(--t-a)" },
    { id: "s", name: "S", tier: "var(--t-s)" },
  ],
  downside: [
    { id: null, name: "Fine", tier: "var(--t-fine)", hint: "Unrated downsides count as fine" },
    { id: "dangerous", name: "Dangerous", tier: "var(--t-danger)", hint: "Gives a warning" },
    { id: "brick", name: "Brick", tier: "var(--t-brick)", hint: "Never take, unless the upside is S tier" },
  ],
};
const LISTS = { upside: "upsides", downside: "downsides" };   // profile keys

let api, state, byId, profile, file;
let dragging = null;                                   // id of the card being dragged
const dropMark = document.createElement("div");         // gold line where the card will land
dropMark.className = "drop-mark";
const filter = { altar: "both", side: "all", target: "all", q: "" };
const $ = id => document.getElementById(id);

const totalWeight = m => Object.values(m.weights).reduce((a, b) => a + b, 0);
const text = m => m.lines.join(" / ");

function visible(m) {
  if (filter.altar !== "both" && !m.altars.includes(filter.altar)) return false;
  if (filter.side !== "all" && m.side !== filter.side) return false;
  if (filter.target !== "all" && m.target !== filter.target) return false;
  if (filter.q && !text(m).toLowerCase().includes(filter.q)) return false;
  return true;
}

// Ids in a column, in priority order. The unrated / fine column holds everything not listed.
function columnIds(side, tier) {
  const lists = profile[LISTS[side]];
  if (tier) return (lists[tier] || []).filter(id => byId[id]);
  const listed = new Set(Object.values(lists).flat());
  return state.mods.filter(m => m.side === side && !listed.has(m.id)).map(m => m.id);
}

function moveCard(id, tier, beforeId) {
  if (profile.readOnly) return;
  const side = byId[id].side;
  const lists = profile[LISTS[side]];
  for (const key of Object.keys(lists)) lists[key] = lists[key].filter(x => x !== id);
  if (tier) {
    const list = lists[tier] || (lists[tier] = []);
    const at = beforeId ? list.indexOf(beforeId) : -1;
    if (at >= 0) list.splice(at, 0, id); else list.push(id);
  }
  render();
  save();
}

async function save() {
  const result = await api.saveProfile(file, { ...profile, modsVersion: state.gameVersion });
  $("saved").textContent = result.ok ? "Saved" : `Not saved: ${result.error}`;
  clearTimeout(save.timer);
  save.timer = setTimeout(() => ($("saved").textContent = ""), 1500);
}

function cardEl(m, colIndex, colCount) {
  const el = document.createElement("div");
  el.className = `card ${m.side}` + (profile.readOnly ? " locked" : "");
  el.dataset.id = m.id;
  el.draggable = !profile.readOnly;
  const w = totalWeight(m);
  const rare = m.side === "upside" && w <= 1000;
  el.innerHTML = `
    <div class="meta">
      ${m.altars.map(a => `<span class="chip altar" style="--c:${ALTAR[a][1]}">${ALTAR[a][0]}</span>`).join("")}
      <span class="chip">${TARGET[m.target]}</span>
      ${rare ? '<span class="chip rare">rare</span>' : ""}
      <span class="w" title="Spawn weight">w ${w.toLocaleString("en-US")}</span>
    </div>
    <div class="txt"></div>
    <div class="moves">
      <button type="button" data-move="-1" aria-label="Move to lower column" ${profile.readOnly || colIndex === 0 ? "disabled" : ""}>‹</button>
      <button type="button" data-move="1" aria-label="Move to higher column" ${profile.readOnly || colIndex === colCount - 1 ? "disabled" : ""}>›</button>
    </div>`;
  el.querySelector(".txt").textContent = text(m);
  el.addEventListener("dragstart", e => {
    e.dataTransfer.setData("text/plain", m.id);
    e.dataTransfer.effectAllowed = "move";
    dragging = m.id;
    requestAnimationFrame(() => el.classList.add("dragging"));   // after the browser took the drag image
  });
  el.addEventListener("dragend", () => {
    dragging = null;
    el.classList.remove("dragging");
    dropMark.remove();
    document.querySelectorAll(".col.over").forEach(c => c.classList.remove("over"));
  });
  el.querySelectorAll("[data-move]").forEach(b => b.addEventListener("click", () => {
    const next = COLS[m.side][colIndex + Number(b.dataset.move)];
    if (next) moveCard(m.id, next.id, null);
  }));
  return el;
}

function renderBoard(side) {
  const wrap = $("cols-" + side);
  const scrolled = [...wrap.querySelectorAll(".list")].map(l => l.scrollTop);   // keep each column where it was
  wrap.innerHTML = "";
  COLS[side].forEach((col, i) => {
    const ids = columnIds(side, col.id).filter(id => visible(byId[id]));
    const colEl = document.createElement("div");
    colEl.className = "col";
    colEl.style.setProperty("--tier", col.tier);
    colEl.innerHTML = `<header><b>${col.name}</b><span>${ids.length}</span></header>${col.hint ? `<div class="hint">${col.hint}</div>` : ""}`;
    const list = document.createElement("div");
    list.className = "list";
    if (!ids.length) list.innerHTML = `<div class="empty">${profile.readOnly ? "No cards" : "Drop cards here"}</div>`;
    ids.forEach(id => list.appendChild(cardEl(byId[id], i, COLS[side].length)));
    // The whole column is the drop target; over its header the card goes to the top.
    colEl.addEventListener("dragover", e => {
      if (profile.readOnly || !dragging || byId[dragging].side !== side) return;
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
      const id = dragging;
      if (!id || byId[id].side !== side) return;
      let next = dropMark.nextElementSibling;
      while (next && (!next.classList.contains("card") || next.classList.contains("dragging"))) next = next.nextElementSibling;
      dropMark.remove();
      moveCard(id, col.id, next ? next.dataset.id : null);
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
  $("board-downside").hidden = filter.side === "upside";
  $("ro").hidden = !profile.readOnly;
  $("del").disabled = profile.readOnly;
  renderBoard("upside");
  renderBoard("downside");
}

async function openProfile(name) {
  if (!state.profiles.some(p => p.file === name)) state.profiles = (await api.getState()).profiles;
  file = name;
  profile = await api.loadProfile(file);
  profile.upsides ||= {};
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
$("q").addEventListener("input", e => { filter.q = e.target.value.trim().toLowerCase(); render(); });
$("profile").addEventListener("change", e => openProfile(e.target.value));
$("dup").addEventListener("click", async () => {
  const name = prompt("Name of the new profile:", `${profile.name} (copy)`);
  if (!name || !name.trim()) return;
  const result = await api.duplicateProfile(file, name.trim());
  state.profiles = result.profiles;
  await openProfile(result.file);
});
$("del").addEventListener("click", async () => {
  if (profile.readOnly || !confirm(`Delete the profile "${profile.name}"?`)) return;
  const result = await api.deleteProfile(file);
  if (!result.ok) return showError(result.error);
  state.profiles = result.profiles;
  await openProfile(result.active);
});

// Overlay settings: written to settings.json right away; the overlay reloads the file by itself.
let hud = {}, hudDefaults = {}, pending = {};

function renderSettings() {
  $("s-font").value = hud.hud_font_size;
  $("o-font").textContent = `${hud.hud_font_size} px`;
  $("s-opacity").value = Math.round(hud.hud_other_opacity * 100);
  $("o-opacity").textContent = `${Math.round(hud.hud_other_opacity * 100)} %`;
  $("s-bottom").value = hud.hud_bottom;
  $("s-offset").value = hud.hud_offset_x;
  document.querySelectorAll(".seg[data-setting]").forEach(seg => seg.querySelectorAll("button").forEach(b =>
    b.setAttribute("aria-pressed", String(b.dataset.v === String(hud[seg.dataset.setting])))));
}

function changeSetting(key, value) {
  hud[key] = value;
  pending[key] = value;
  renderSettings();
  clearTimeout(changeSetting.timer);   // sliders fire many events; write once they settle
  changeSetting.timer = setTimeout(async () => {
    const result = await api.saveSettings(pending);
    pending = {};
    hud = result.current;
    renderSettings();
    $("s-saved").textContent = "Saved";
    clearTimeout(changeSetting.done);
    changeSetting.done = setTimeout(() => ($("s-saved").textContent = ""), 1500);
  }, 150);
}

function showSettingsPanel(open) {
  $("hud").hidden = !open;
  $("hud-toggle").setAttribute("aria-expanded", String(open));
  try { localStorage.setItem("overlaySettingsOpen", open ? "1" : ""); } catch { /* storage may be blocked */ }
}
$("hud-toggle").addEventListener("click", () => showSettingsPanel($("hud").hidden));
try { showSettingsPanel(localStorage.getItem("overlaySettingsOpen") === "1"); } catch { showSettingsPanel(false); }
$("s-font").addEventListener("input", e => changeSetting("hud_font_size", Number(e.target.value)));
$("s-opacity").addEventListener("input", e => changeSetting("hud_other_opacity", Number(e.target.value) / 100));
$("s-bottom").addEventListener("change", e => changeSetting("hud_bottom", Number(e.target.value)));
$("s-offset").addEventListener("change", e => changeSetting("hud_offset_x", Number(e.target.value)));
document.querySelectorAll(".seg[data-setting]").forEach(seg => seg.addEventListener("click", e => {
  const b = e.target.closest("button");
  if (!b) return;
  const v = b.dataset.v;
  changeSetting(seg.dataset.setting, v === "true" ? true : v === "false" ? false : v);
}));
$("s-reset").addEventListener("click", () => Object.entries(hudDefaults).forEach(([k, v]) => changeSetting(k, v)));

(async () => {
  try {
    api = await connectBackend();
    state = await api.getState();
    byId = Object.fromEntries(state.mods.map(m => [m.id, m]));
    $("subtitle").textContent = `${state.mods.length} mods · game ${state.gameVersion} · changes apply to the overlay instantly`;
    await openProfile(state.active);
    ({ current: hud, defaults: hudDefaults } = await api.getSettings());
    renderSettings();
  } catch (e) {
    $("subtitle").textContent = "";
    showError(e.message);
  }
})();
