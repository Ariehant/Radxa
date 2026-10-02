export type Theme = "system" | "light" | "dark";

/** `system` defers to prefers-color-scheme (see app.css); light/dark force it. */
export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
  try {
    localStorage.setItem("nexus.theme", theme);
  } catch {
    /* ignore */
  }
}

/** Apply the last theme before first paint to avoid a flash. */
export function restoreTheme() {
  try {
    const t = localStorage.getItem("nexus.theme");
    if (t === "light" || t === "dark" || t === "system") applyTheme(t);
  } catch {
    /* ignore */
  }
}
