import { getCurrentWindow } from "@tauri-apps/api/window";

/** Wires the custom titlebar of the frameless window. No-op in a plain browser. */
export function setupTitlebar() {
  const controls = document.getElementById("titlebar-controls");
  if (!("__TAURI_INTERNALS__" in window)) {
    if (controls) controls.style.display = "none";
    return;
  }

  const win = getCurrentWindow();

  document.getElementById("win-min")?.addEventListener("click", () => win.minimize());
  document.getElementById("win-max")?.addEventListener("click", async () => {
    await win.toggleMaximize();
    updateMaxIcon();
  });
  document.getElementById("win-close")?.addEventListener("click", () => win.close());

  // 双击标题栏最大化/还原
  document.getElementById("titlebar")?.addEventListener("dblclick", (e) => {
    if ((e.target as HTMLElement).closest(".titlebar-btn")) return;
    win.toggleMaximize().then(updateMaxIcon);
  });

  const updateMaxIcon = async () => {
    const icon = document.querySelector("#win-max i");
    if (!icon) return;
    const maximized = await win.isMaximized();
    icon.className = maximized ? "fa-regular fa-clone" : "fa-regular fa-square";
  };
}
