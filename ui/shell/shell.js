// App shell for every main-window page: the left sidebar and `window.pov`.
// Load with `defer` after shell.css. `pov.listen` passes Tauri's event object (`e.payload`).
// Pages of other windows set `<body data-sidebar="none">` to get `window.pov` without the sidebar.
(function () {
  "use strict";

  const tauri = window.__TAURI__;
  const missing = () => Promise.reject(new Error("not running inside Path of Vision"));
  window.pov = {
    invoke: (cmd, args) => (tauri ? tauri.core.invoke(cmd, args) : missing()),
    listen: (event, handler) => (tauri ? tauri.event.listen(event, handler) : missing()),
  };

  const PAGES = [
    { name: "Home", href: "/shell/home.html" },
    { name: "Altars", href: "/modules/altar/board/index.html" },
  ];

  function sidebar() {
    const nav = document.createElement("nav");
    nav.className = "pov-sidebar";
    nav.setAttribute("aria-label", "Pages");

    const brand = document.createElement("p");
    brand.className = "brand";
    brand.textContent = "Path of Vision";
    nav.append(brand);

    for (const page of PAGES) {
      const a = document.createElement("a");
      a.href = page.href;
      a.textContent = page.name;
      if (location.pathname === page.href) a.setAttribute("aria-current", "page");
      nav.append(a);
    }

    const spacer = document.createElement("div");
    spacer.className = "spacer";
    nav.append(spacer);

    const gear = document.createElement("button");
    gear.type = "button";
    gear.className = "gear";
    gear.title = "Settings";
    gear.setAttribute("aria-label", "Settings");
    gear.textContent = "⚙";
    gear.addEventListener("click", () => {
      window.pov.invoke("open_settings", { section: null }).catch((e) => console.error(e));
    });
    nav.append(gear);
    return nav;
  }

  if (document.body.dataset.sidebar === "none") return;
  document.body.prepend(sidebar());
  document.body.classList.add("has-sidebar");
})();
