// Frontend ↔ Bibliothek: alle echten Operationen laufen über Tauri-Commands (src-tauri),
// die dünn auf servercontrol-core sitzen. Hier nur UI-Zustand und Aufrufe.
const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);

function show(view) {
  $("connect-view").hidden = view !== "connect";
  $("overview-view").hidden = view !== "overview";
}

function setStatus(overview) {
  const badge = $("status-badge");
  if (!overview) {
    badge.textContent = "nicht verbunden";
    badge.className = "badge badge-off";
    return;
  }
  if (overview.online) {
    badge.textContent = "läuft";
    badge.className = "badge badge-on";
  } else {
    badge.textContent = "gestoppt";
    badge.className = "badge badge-stopped";
  }
}

function renderOverview(o) {
  $("ov-state").textContent = o.online ? "Online" : "Offline";
  $("ov-version").textContent = o.version ? `Version ${o.version}` : "";
  $("ov-mods").textContent = o.mod_total;
  $("ov-mods-sub").textContent = `${o.mod_active} aktiv · ${o.mod_inactive} inaktiv`;
  $("ov-dlc").textContent = o.mod_dlc;
  setStatus(o);
}

async function connect(ev) {
  ev.preventDefault();
  const btn = $("btn-connect");
  const err = $("connect-error");
  err.hidden = true;
  btn.disabled = true;
  btn.textContent = "Verbinde…";
  try {
    const overview = await invoke("connect", {
      url: $("in-url").value.trim(),
      username: $("in-user").value.trim(),
    });
    $("server-name").textContent = $("in-url").value.trim();
    renderOverview(overview);
    show("overview");
  } catch (e) {
    err.textContent = String(e);
    err.hidden = false;
  } finally {
    btn.disabled = false;
    btn.textContent = "Verbinden";
  }
}

async function refresh() {
  const btn = $("btn-refresh");
  btn.disabled = true;
  try {
    renderOverview(await invoke("overview"));
  } catch (e) {
    alert("Aktualisieren fehlgeschlagen: " + e);
  } finally {
    btn.disabled = false;
  }
}

async function disconnect() {
  try {
    await invoke("disconnect");
  } catch (_) {}
  $("server-name").textContent = "— nicht verbunden —";
  setStatus(null);
  show("connect");
}

window.addEventListener("DOMContentLoaded", () => {
  $("connect-form").addEventListener("submit", connect);
  $("btn-refresh").addEventListener("click", refresh);
  $("btn-disconnect").addEventListener("click", disconnect);
  show("connect");
});
