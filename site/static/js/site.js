// Small enhancements. Everything on the page works without this file.
(function () {
  "use strict";
  var root = document.documentElement;
  var platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
  var isMac = /mac|iphone|ipad/i.test(platform);
  var isWindows = /win/i.test(platform);

  // Theme switch: Noite and Dia, the names BIA's own themes use.
  var themeSwitch = document.querySelector("[data-theme-switch]");
  if (themeSwitch) {
    var dark = window.matchMedia("(prefers-color-scheme: dark)");
    var buttons = themeSwitch.querySelectorAll("[data-set-theme]");
    var current = function () { return root.dataset.theme || (dark.matches ? "noite" : "dia"); };
    var render = function () {
      buttons.forEach(function (b) { b.setAttribute("aria-pressed", String(b.dataset.setTheme === current())); });
    };
    buttons.forEach(function (b) {
      b.addEventListener("click", function () {
        window.biaTheme(b.dataset.setTheme);
        try { localStorage.setItem("bia-theme", b.dataset.setTheme); } catch (e) {}
        render();
      });
    });
    dark.addEventListener("change", render);
    themeSwitch.hidden = false;
    render();
  }

  // On a Mac, show ⌘ where the page says Ctrl.
  if (isMac) {
    document.querySelectorAll("kbd[data-mod]").forEach(function (k) {
      k.textContent = k.textContent.replace("Ctrl+", "⌘");
    });
  }

  // Install command: tabs per system, preselected from the visitor's OS.
  document.querySelectorAll("[data-install]").forEach(function (box) {
    var tablist = box.querySelector("[role=tablist]");
    var tabs = Array.prototype.slice.call(box.querySelectorAll("[role=tab]"));
    var panels = box.querySelectorAll("[role=tabpanel]");
    var select = function (tab, focus) {
      tabs.forEach(function (t) {
        var on = t === tab;
        t.setAttribute("aria-selected", String(on));
        t.tabIndex = on ? 0 : -1;
      });
      panels.forEach(function (p) { p.hidden = p.dataset.os !== tab.dataset.os; });
      if (focus) tab.focus();
    };
    tabs.forEach(function (t, i) {
      t.addEventListener("click", function () { select(t, false); });
      t.addEventListener("keydown", function (e) {
        var next = e.key === "ArrowRight" ? i + 1 : e.key === "ArrowLeft" ? i - 1 : null;
        if (next === null) return;
        e.preventDefault();
        select(tabs[(next + tabs.length) % tabs.length], true);
      });
    });
    tablist.hidden = false;
    select(tabs[isWindows ? 1 : 0], false);
  });

  // Copy button: copies the command in the visible panel.
  var status = document.querySelector("[data-copy-status]");
  document.querySelectorAll("[data-install] [data-copy]").forEach(function (button) {
    if (!navigator.clipboard) return;
    var box = button.closest("[data-install]");
    var label = button.textContent;
    var timer;
    button.hidden = false;
    button.addEventListener("click", function () {
      var code = box.querySelector("[role=tabpanel]:not([hidden]) code");
      navigator.clipboard.writeText(code.textContent).then(function () {
        button.textContent = status.dataset.done;
        status.textContent = status.dataset.done;
      }, function () {
        button.textContent = status.dataset.failed;
        window.getSelection().selectAllChildren(code);
      });
      clearTimeout(timer);
      timer = setTimeout(function () { button.textContent = label; status.textContent = ""; }, 2000);
    });
  });

  // Docs menu: open beside the page on wide screens, folded on narrow ones.
  var menu = document.querySelector("[data-docs-menu]");
  if (menu) {
    var wide = window.matchMedia("(min-width: 60.0625rem)");
    var sync = function () { menu.open = wide.matches; };
    wide.addEventListener("change", sync);
    sync();
  }
})();
