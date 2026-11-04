// Home: runner status and the module switches.
(function () {
  "use strict";

  const $ = (id) => document.getElementById(id);
  const POE = {
    not_found: ["not running", "off"],
    background: ["in background", ""],
    foreground: ["in front", "on"],
  };

  function showStatus(s) {
    const [text, dot] = POE[s.poe] || [s.poe, ""];
    $("poe").textContent = text;
    $("poe-dot").className = "dot " + dot;
    $("area").textContent = s.area || "unknown";
    $("status-text").textContent = s.text;
    showResolution(s);
  }

  /** The resolution note under the status line; nothing for verified sizes. */
  function showResolution(s) {
    const box = $("resolution");
    let text = "";
    if (s.resolution === "unsupported" && s.client_size) {
      const [w, h] = s.client_size;
      text = `Unsupported game size ${w}×${h} – enable 'Try unsupported resolutions' in Settings → General to try anyway`;
    } else if (s.resolution === "untested") {
      text = "Untested resolution";
    }
    box.textContent = text;
    box.hidden = !text;
  }

  function showError(e) {
    const box = $("error");
    box.textContent = e ? "Could not save the setting: " + e : "";
    box.hidden = !e;
  }

  const toggle = $("altar-enabled");
  function showSettings(settings) {
    const on = !!(settings.modules && settings.modules.altar && settings.modules.altar.enabled);
    toggle.checked = on;
    toggle.disabled = false;
    $("altar-state").textContent = on ? "On" : "Off";
  }

  toggle.addEventListener("change", () => {
    const wanted = toggle.checked;
    toggle.disabled = true;
    window.pov
      .invoke("settings_set", { section: "altar", key: "enabled", value: wanted })
      .then((s) => {
        showError(null);
        showSettings(s);
      })
      .catch((e) => {
        showError(String(e));
        toggle.checked = !wanted;
        toggle.disabled = false;
        $("altar-state").textContent = toggle.checked ? "On" : "Off";
      });
  });

  let pendingOffer = null;
  let version = "";

  /** The version card: the running version, or the pending update offer (PoE was running at start). */
  function showUpdate(offer) {
    pendingOffer = offer;
    const card = $("version-card");
    card.classList.toggle("offer", !!offer);
    $("version-h").textContent = offer ? "Update" : "Version";
    $("version-text").textContent = offer
      ? (version ? `v${version} → ` : "") + "v" + offer.version
      : version ? "v" + version : "…";
    $("up-to-date").hidden = !!offer || !version;
    $("update-box").hidden = !offer;
  }

  /** Update button states: ready, installing (spinner) or failed (with the error). */
  function showInstall(installing, error) {
    const btn = $("update-now");
    btn.disabled = installing;
    btn.innerHTML = installing ? '<span class="spinner" aria-hidden="true"></span>Installing…' : error ? "Try again" : "Update now";
    $("update-ready").hidden = installing || !!error;
    $("update-restart").hidden = !installing;
    $("update-error").textContent = error ? "Update failed: " + error : "";
    $("update-error").hidden = !error;
  }

  $("update-now").addEventListener("click", () => {
    showInstall(true, null);
    window.pov.invoke("update_now").catch((e) => {
      showInstall(false, String(e));
      showUpdate(pendingOffer);
    });
  });

  window.pov.invoke("app_version").then(
    (v) => {
      version = v;
      showUpdate(pendingOffer);
    },
    () => {}
  );
  window.pov.listen("app:update", (e) => showUpdate(e.payload));
  window.pov.invoke("update_offer").then(showUpdate, () => {});

  window.pov.listen("app:status", (e) => showStatus(e.payload));
  window.pov.listen("settings:changed", (e) => showSettings(e.payload));
  window.pov.invoke("app_status").then(showStatus, (e) => ($("status-text").textContent = String(e)));
  window.pov.invoke("settings_get").then(showSettings, (e) => showError(String(e)));
})();
