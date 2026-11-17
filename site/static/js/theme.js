// Runs before the first paint: applies a saved theme so the page never
// flashes the wrong one. Without a saved choice, CSS follows the system.
(function () {
  var root = document.documentElement;
  root.classList.add("js");
  // Points the code highlighting at the chosen theme, or back at the system.
  window.biaTheme = function (theme) {
    var dia = document.getElementById("code-dia");
    var noite = document.getElementById("code-noite");
    if (theme) root.dataset.theme = theme;
    if (!dia || !noite) return;
    dia.media = theme ? (theme === "dia" ? "all" : "not all") : "(prefers-color-scheme: light)";
    noite.media = theme ? (theme === "noite" ? "all" : "not all") : "(prefers-color-scheme: dark)";
  };
  try {
    var saved = localStorage.getItem("bia-theme");
    if (saved === "noite" || saved === "dia") window.biaTheme(saved);
  } catch (e) {}
})();
