// Frontend ↔ Bibliothek: alle echten Operationen laufen über Tauri-Commands (src-tauri),
// die dünn auf servercontrol-core sitzen. Hier nur UI-Zustand und Aufrufe.
const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);

// --- Mods (G2, Pflichtenheft 7.5) ---
const modsState = {
  all: [], // ServerMod[] wie von mods_view geliefert
  online: false,
  filter: "all",
  query: "",
  sortKey: "name",
  sortDir: "asc",
  selected: new Set(), // file_name
};

const STATUS_LABEL = { Active: "Aktiv", Inactive: "Inaktiv", Orphan: "Karteileiche" };
const STATUS_CLASS = { Active: "active", Inactive: "inactive", Orphan: "orphan" };
const STATUS_ORDER = { Active: 0, Inactive: 1, Orphan: 2 };

function fmtSize(bytes) {
  if (!bytes) return "—";
  const mb = bytes / (1024 * 1024);
  return mb >= 1000 ? (mb / 1024).toFixed(2) + " GB" : mb.toFixed(1) + " MB";
}

// Einheitlicher Bestätigungsdialog für eingreifende Aktionen (7.10).
function confirmDialog(title, body, okLabel, danger) {
  return new Promise((resolve) => {
    $("confirm-title").textContent = title;
    $("confirm-body").textContent = body;
    const ok = $("confirm-ok");
    ok.textContent = okLabel;
    ok.className = danger ? "primary-btn danger" : "primary-btn";
    const modal = $("confirm-modal");
    modal.hidden = false;
    const cleanup = (result) => {
      modal.hidden = true;
      ok.removeEventListener("click", onOk);
      $("confirm-cancel").removeEventListener("click", onCancel);
      resolve(result);
    };
    const onOk = () => cleanup(true);
    const onCancel = () => cleanup(false);
    ok.addEventListener("click", onOk);
    $("confirm-cancel").addEventListener("click", onCancel);
  });
}

function modsVisible() {
  const q = modsState.query;
  let list = modsState.all.filter((m) => {
    if (modsState.filter !== "all" && m.status !== modsState.filter) return false;
    if (!q) return true;
    const name = (m.display_name || "").toLowerCase();
    return name.includes(q) || m.file_name.toLowerCase().includes(q);
  });
  const key = modsState.sortKey;
  const cmp = (a, b) => {
    switch (key) {
      case "status":
        return (
          (STATUS_ORDER[a.status] - STATUS_ORDER[b.status]) ||
          (a.display_name || "").localeCompare(b.display_name || "")
        );
      case "ver":
        return (a.version || "").localeCompare(b.version || "", undefined, { numeric: true });
      case "author":
        return (a.author || "").localeCompare(b.author || "");
      case "file":
        return a.file_name.localeCompare(b.file_name);
      case "size":
        return (a.size || 0) - (b.size || 0);
      default:
        return (a.display_name || a.file_name).localeCompare(b.display_name || b.file_name);
    }
  };
  list.sort((a, b) => (modsState.sortDir === "asc" ? cmp(a, b) : -cmp(a, b)));
  return list;
}

function renderModsCounts() {
  const active = modsState.all.filter((m) => m.status === "Active").length;
  const inactive = modsState.all.filter((m) => m.status === "Inactive").length;
  const orphan = modsState.all.filter((m) => m.status === "Orphan").length;
  $("mods-counts").innerHTML =
    `<span><b>${active}</b> aktiv</span>` +
    `<span><b>${inactive}</b> inaktiv</span>` +
    `<span><b>${orphan}</b> Karteileichen</span>`;
}

function renderModsHeaders() {
  document.querySelectorAll("#mods-view th.sortable").forEach((th) => {
    const active = th.dataset.k === modsState.sortKey;
    th.classList.toggle("sorted", active);
    th.querySelector(".arrow").textContent = active
      ? modsState.sortDir === "asc"
        ? "▲"
        : "▼"
      : "";
  });
}

function renderModsActionbar() {
  const bar = $("mods-actionbar");
  const n = modsState.selected.size;
  bar.hidden = n === 0;
  $("mods-sel-count").textContent = n;
  const locked = modsState.online;
  $("mods-btn-activate").disabled = n === 0 || locked;
  $("mods-btn-deactivate").disabled = n === 0 || locked;
  $("mods-btn-delete-sel").disabled = n === 0 || locked;
}

function renderMods() {
  $("mods-banner").hidden = !modsState.online;
  const rows = $("mods-rows");
  const list = modsVisible();
  const locked = modsState.online;
  rows.innerHTML = list
    .map((m) => {
      const checked = modsState.selected.has(m.file_name) ? "checked" : "";
      const name = m.display_name || m.file_name;
      const author = m.author || "—";
      return `<tr data-file="${m.file_name}">
        <td class="c-sel"><input type="checkbox" class="rowchk" ${checked} ${locked ? "disabled" : ""} aria-label="${name} auswählen" /></td>
        <td><span class="badge badge-${STATUS_CLASS[m.status]}">${STATUS_LABEL[m.status]}</span></td>
        <td>${name}</td>
        <td class="c-ver">${m.version || "—"}</td>
        <td>${author}</td>
        <td class="c-file">${m.file_name}</td>
        <td class="c-size">${fmtSize(m.size)}</td>
        <td class="c-dlc">${m.is_dlc ? '<span class="dlc-tag">DLC</span>' : "—"}</td>
        <td class="c-del">
          <button class="ghost danger small btn-del-mod" ${locked ? "disabled" : ""} title="Mod löschen">Löschen</button>
        </td>
      </tr>`;
    })
    .join("");
  renderModsCounts();
  renderModsHeaders();
  renderModsActionbar();
  $("mods-sel-all").checked = list.length > 0 && list.every((m) => modsState.selected.has(m.file_name));
  $("mods-sel-all").disabled = locked;
}

async function loadMods() {
  const err = $("mods-error");
  err.hidden = true;
  try {
    const view = await invoke("mods_view");
    modsState.all = view.mods;
    modsState.online = view.online;
    modsState.selected.clear();
    renderMods();
    setStatus({ online: view.online });
  } catch (e) {
    err.textContent = String(e);
    err.hidden = false;
  }
}

async function applyModsAction(activate) {
  const files = [...modsState.selected];
  if (files.length === 0) return;
  const verb = activate ? "aktiviert" : "deaktiviert";
  const ok = await confirmDialog(
    activate ? "Mods aktivieren?" : "Mods deaktivieren?",
    `${files.length} Mod(s) werden ${verb}.`,
    activate ? "Aktivieren" : "Deaktivieren",
    false,
  );
  if (!ok) return;
  try {
    await invoke("set_active", {
      activate: activate ? files : [],
      deactivate: activate ? [] : files,
    });
    await loadMods();
  } catch (e) {
    $("mods-error").textContent = String(e);
    $("mods-error").hidden = false;
  }
}

async function deleteMods(files) {
  if (files.length === 0) return;
  const ok = await confirmDialog(
    "Mod löschen?",
    files.length === 1
      ? "Die Mod-Datei wird vom Server gelöscht."
      : `${files.length} Mod-Dateien werden vom Server gelöscht.`,
    "Löschen",
    true,
  );
  if (!ok) return;
  try {
    for (const f of files) {
      await invoke("delete_mod", { fileName: f });
    }
    await loadMods();
  } catch (e) {
    $("mods-error").textContent = String(e);
    $("mods-error").hidden = false;
  }
}

function initModsView() {
  $("mods-search").addEventListener("input", (e) => {
    modsState.query = e.target.value.toLowerCase().trim();
    renderMods();
  });
  $("mods-filters").addEventListener("click", (e) => {
    const b = e.target.closest("button");
    if (!b) return;
    modsState.filter = b.dataset.f;
    [...e.currentTarget.children].forEach((c) => c.classList.toggle("on", c === b));
    renderMods();
  });
  document.querySelector("#mods-view thead").addEventListener("click", (e) => {
    const th = e.target.closest("th.sortable");
    if (!th) return;
    const k = th.dataset.k;
    if (k === modsState.sortKey) modsState.sortDir = modsState.sortDir === "asc" ? "desc" : "asc";
    else {
      modsState.sortKey = k;
      modsState.sortDir = k === "size" ? "desc" : "asc";
    }
    renderMods();
  });
  $("mods-rows").addEventListener("change", (e) => {
    if (!e.target.classList.contains("rowchk")) return;
    const file = e.target.closest("tr").dataset.file;
    if (e.target.checked) modsState.selected.add(file);
    else modsState.selected.delete(file);
    renderModsActionbar();
    $("mods-sel-all").checked = modsVisible().every((m) => modsState.selected.has(m.file_name));
  });
  $("mods-rows").addEventListener("click", (e) => {
    if (!e.target.classList.contains("btn-del-mod")) return;
    const file = e.target.closest("tr").dataset.file;
    deleteMods([file]);
  });
  $("mods-sel-all").addEventListener("change", (e) => {
    const vis = modsVisible();
    if (e.target.checked) vis.forEach((m) => modsState.selected.add(m.file_name));
    else vis.forEach((m) => modsState.selected.delete(m.file_name));
    renderMods();
  });
  $("btn-mods-refresh").addEventListener("click", loadMods);
  $("mods-btn-activate").addEventListener("click", () => applyModsAction(true));
  $("mods-btn-deactivate").addEventListener("click", () => applyModsAction(false));
  $("mods-btn-delete-sel").addEventListener("click", () => deleteMods([...modsState.selected]));
}

// --- Spieleinstellungen (G6, Pflichtenheft 7.9) ---
let settingsView = { online: false, settings: null, options: null, summary: null };

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function populateSelect(id, options, selectedValue) {
  const el = $(id);
  el.innerHTML = (options || [])
    .map((o) => `<option value="${o.value}">${o.label}</option>`)
    .join("");
  el.value = String(selectedValue);
}

function statsIntervalHint(seconds) {
  if (!seconds) return "";
  const days = seconds / 86400;
  if (days >= 1) {
    const rounded = Math.round(days);
    const note = days >= 30 ? " — Feed praktisch aus" : "";
    return `≈ ${rounded} Tag${rounded === 1 ? "" : "e"}${note}`;
  }
  const hours = seconds / 3600;
  return hours >= 1 ? `≈ ${Math.round(hours)} Std.` : "";
}

// Neues-Spiel-Felder (Map/Startgeld/Kredit/Schwierigkeit) sind nur bei einem **leeren**
// Savegame-Slot editierbar — der Server erkennt das am Options-Text "- Empty" (Kap. 6,
// checkSavegame). Rein optische Sperre hier; der eigentliche Server verifiziert beim Speichern.
function updateNewGameFieldsLock() {
  const opt = $("set-savegame").selectedOptions[0];
  const empty = !!opt && opt.textContent.includes("- Empty");
  ["set-map", "set-money", "set-loan", "set-difficulty"].forEach((id) => ($(id).disabled = !empty));
  $("set-savegame-hint").textContent = empty
    ? "Leerer Slot — Startbedingungen können gesetzt werden."
    : "Belegter Spielstand — Startbedingungen sind gesperrt.";
}

function renderSettingsSummary(rows) {
  $("settings-summary-rows").innerHTML = rows
    .map((row) => {
      if (row.is_secret) {
        return `<tr><th>${escapeHtml(row.label)}</th><td>
          <span class="secret-value" data-value="${escapeHtml(row.value)}">••••••••</span>
          <button type="button" class="ghost small btn-reveal">Anzeigen</button>
        </td></tr>`;
      }
      return `<tr><th>${escapeHtml(row.label)}</th><td>${escapeHtml(row.value)}</td></tr>`;
    })
    .join("");
}

function renderSettings() {
  const { online, settings, options, summary } = settingsView;
  $("settings-banner").hidden = !online;
  $("settings-summary-wrap").hidden = !online || !summary;
  $("settings-form").hidden = online || !settings;
  if (online) {
    if (summary) renderSettingsSummary(summary);
    return;
  }
  if (!settings) return;

  $("set-name").value = settings.game_name;
  $("set-admin-pass").value = settings.admin_password;
  $("set-game-pass").value = settings.game_password;

  populateSelect("set-savegame", options.savegames, settings.savegame);
  populateSelect("set-map", options.maps, settings.map_start);
  populateSelect("set-money", options.initial_money, settings.initial_money);
  populateSelect("set-loan", options.initial_loan, settings.initial_loan);
  populateSelect("set-difficulty", options.economic_difficulty, settings.economic_difficulty);
  updateNewGameFieldsLock();

  $("set-port").value = settings.server_port;
  populateSelect("set-max-player", options.max_player, settings.max_player);
  populateSelect("set-language", options.mp_language, settings.mp_language);
  $("set-crossplay").checked = settings.crossplay_allowed;

  $("set-autosave").value = settings.auto_save_interval;
  $("set-stats-interval").value = settings.stats_interval;
  $("set-stats-hint").textContent = statsIntervalHint(settings.stats_interval);
  populateSelect("set-pause", options.pause_game_if_empty, settings.pause_game_if_empty);
}

async function loadSettings() {
  const err = $("settings-error");
  err.hidden = true;
  try {
    settingsView = await invoke("settings_view");
    renderSettings();
  } catch (e) {
    err.textContent = String(e);
    err.hidden = false;
  }
}

async function saveSettingsForm(ev) {
  ev.preventDefault();
  const err = $("settings-error");
  err.hidden = true;
  const settings = {
    game_name: $("set-name").value.trim(),
    admin_password: $("set-admin-pass").value,
    game_password: $("set-game-pass").value,
    savegame: parseInt($("set-savegame").value, 10),
    map_start: $("set-map").value,
    initial_money: parseInt($("set-money").value, 10),
    initial_loan: parseInt($("set-loan").value, 10),
    economic_difficulty: parseInt($("set-difficulty").value, 10),
    server_port: parseInt($("set-port").value, 10),
    max_player: parseInt($("set-max-player").value, 10),
    mp_language: $("set-language").value,
    auto_save_interval: parseInt($("set-autosave").value, 10),
    stats_interval: parseInt($("set-stats-interval").value, 10),
    pause_game_if_empty: parseInt($("set-pause").value, 10),
    crossplay_allowed: $("set-crossplay").checked,
  };
  const btn = $("btn-settings-save");
  btn.disabled = true;
  try {
    await invoke("save_settings", { settings });
    await loadSettings();
  } catch (e) {
    err.textContent = String(e);
    err.hidden = false;
  } finally {
    btn.disabled = false;
  }
}

function initSettingsView() {
  $("btn-settings-refresh").addEventListener("click", loadSettings);
  $("settings-form").addEventListener("submit", saveSettingsForm);
  $("set-savegame").addEventListener("change", updateNewGameFieldsLock);
  $("settings-summary-rows").addEventListener("click", (e) => {
    const btn = e.target.closest(".btn-reveal");
    if (!btn) return;
    const span = btn.previousElementSibling;
    const showing = btn.textContent === "Verbergen";
    span.textContent = showing ? "••••••••" : span.dataset.value;
    btn.textContent = showing ? "Anzeigen" : "Verbergen";
  });
}

// --- Serverprofile (G1, Pflichtenheft 7.4) ---
let profiles = []; // ProfileDto[] (ohne Passwort) aus list_profiles
let activeProfileId = null; // Profil-ID der aktuell verbundenen Sitzung, sonst null
let editingProfileId = null; // Profil, das gerade im Editor steht (neu = eigene, ungespeicherte ID)

function show(view) {
  $("empty-view").hidden = view !== "empty";
  $("overview-view").hidden = view !== "overview";
  $("mods-view").hidden = view !== "mods";
  $("settings-view").hidden = view !== "settings";
  $("mods-actionbar").hidden = view !== "mods" || modsState.selected.size === 0;
  document
    .querySelectorAll(".nav-item")
    .forEach((b) => b.classList.toggle("active", b.dataset.view === view));
  if (view === "mods") loadMods();
  if (view === "settings") loadSettings();
}

function updateNavLocks() {
  const connected = activeProfileId !== null;
  document.querySelectorAll(".nav-item").forEach((b) => {
    if (b.dataset.view !== "overview") b.disabled = !connected;
  });
}

async function loadProfiles() {
  profiles = await invoke("list_profiles");
  renderServerMenu();
  renderProfileList();
}

function renderServerMenu() {
  const list = $("server-menu-list");
  list.innerHTML = profiles
    .map((p) => {
      const isActive = p.id === activeProfileId;
      return `<button class="menu-item ${isActive ? "current" : ""}" data-id="${p.id}">
        <span class="m-dot ${isActive ? "on" : ""}"></span>${p.name}
        <span class="m-sub">${isActive ? "verbunden" : "nicht verbunden"}</span>
      </button>`;
    })
    .join("");
  $("menu-disconnect").hidden = activeProfileId === null;
}

function toggleServerMenu(open) {
  const menu = $("server-menu");
  const willOpen = open ?? menu.hidden;
  menu.hidden = !willOpen;
  $("server-pick").setAttribute("aria-expanded", String(willOpen));
}

// --- Serverprofile: Verwaltungsbildschirm ---

function renderProfileList() {
  $("profile-list").innerHTML = profiles
    .map((p) => {
      const isActive = p.id === activeProfileId;
      return `<button class="pcard ${p.id === editingProfileId ? "active" : ""}" data-id="${p.id}">
        <span class="pn"><span class="m-dot ${isActive ? "on" : ""}"></span>${p.name}</span>
        <span class="pa">${p.base_url}</span>
        <span class="ps">${isActive ? "verbunden" : "nicht verbunden"}${p.file_access ? " · FTP/SFTP" : ""}</span>
      </button>`;
    })
    .join("");
}

function splitUrl(base_url) {
  try {
    const u = new URL(base_url);
    return { proto: u.protocol.replace(":", ""), addr: base_url.slice(u.protocol.length + 2) };
  } catch (_) {
    return { proto: "http", addr: base_url || "" };
  }
}

function toggleFtpSection() {
  $("ftp-body").hidden = !$("pf-ftp").checked;
}

// Nimmt dem Adressfeld ein versehentlich mitgetipptes/eingefügtes Schema ab ("http://host…",
// "https//host…" o. Ä.) und übernimmt es in die Protokoll-Auswahl — sonst würde beim Speichern
// ein doppeltes Schema landen ("http://http://…").
function stripSchemeFromAddr() {
  const input = $("pf-addr");
  const m = input.value.trim().match(/^(https?):?\/\/?\s*/i);
  if (!m) return;
  $("pf-proto").value = m[1].toLowerCase();
  input.value = input.value.trim().slice(m[0].length);
}

function loadProfileIntoForm(id) {
  editingProfileId = id;
  const p = profiles.find((x) => x.id === id);
  $("edit-title").textContent = "Profil bearbeiten";
  $("profile-error").hidden = true;
  const { proto, addr } = splitUrl(p.base_url);
  $("pf-name").value = p.name;
  $("pf-proto").value = proto;
  $("pf-addr").value = addr;
  $("pf-user").value = p.username;
  $("pf-pass").value = "";
  $("pf-pass").placeholder = p.has_password
    ? "•••• gespeichert — zum Ändern neu eingeben"
    : "Passwort eingeben";
  $("pf-cert").checked = !!p.accept_invalid_cert;
  $("pf-ftp").checked = !!p.file_access;
  toggleFtpSection();
  $("pf-ftp-proto").value = p.file_access?.protocol || "ftp";
  $("pf-ftp-host").value = p.file_access?.host || "";
  $("pf-ftp-port").value = p.file_access?.port || "";
  $("pf-ftp-user").value = p.file_access?.username || "";
  $("pf-ftp-pass").value = "";
  $("pf-ftp-pass").placeholder = p.has_ftp_password ? "•••• gespeichert" : "Passwort eingeben";
  $("pf-ftp-mods-path").value = p.file_access?.mods_path || "";
  $("pf-delete").hidden = false;
  $("pf-connect").hidden = p.id === activeProfileId;
  renderProfileList();
}

function newProfileForm() {
  editingProfileId = crypto.randomUUID();
  $("edit-title").textContent = "Neues Profil";
  $("profile-error").hidden = true;
  $("pf-name").value = "";
  $("pf-proto").value = "http";
  $("pf-addr").value = "";
  $("pf-user").value = "admin";
  $("pf-pass").value = "";
  $("pf-pass").placeholder = "Passwort eingeben";
  $("pf-cert").checked = false;
  $("pf-ftp").checked = false;
  toggleFtpSection();
  $("pf-ftp-proto").value = "ftp";
  $("pf-ftp-host").value = "";
  $("pf-ftp-port").value = "";
  $("pf-ftp-user").value = "";
  $("pf-ftp-pass").value = "";
  $("pf-ftp-pass").placeholder = "Passwort eingeben";
  $("pf-ftp-mods-path").value = "";
  $("pf-delete").hidden = true;
  $("pf-connect").hidden = true;
  renderProfileList();
}

function openManage() {
  $("manage-view").hidden = false;
  loadProfiles().then(() => {
    if (profiles.length > 0) loadProfileIntoForm(profiles[0].id);
    else newProfileForm();
  });
}

function closeManage() {
  $("manage-view").hidden = true;
}

async function saveProfileForm(ev) {
  ev.preventDefault();
  const err = $("profile-error");
  err.hidden = true;
  stripSchemeFromAddr();
  const ftpEnabled = $("pf-ftp").checked;
  const profile = {
    id: editingProfileId,
    name: $("pf-name").value.trim(),
    base_url: `${$("pf-proto").value}://${$("pf-addr").value.trim()}`,
    username: $("pf-user").value.trim(),
    credential_key: "",
    accept_invalid_cert: $("pf-cert").checked,
    file_access: ftpEnabled
      ? {
          protocol: $("pf-ftp-proto").value,
          host: $("pf-ftp-host").value.trim(),
          port: parseInt($("pf-ftp-port").value, 10) || 0,
          username: $("pf-ftp-user").value.trim(),
          credential_key: "",
          mods_path: $("pf-ftp-mods-path").value.trim(),
        }
      : null,
  };
  try {
    const dto = await invoke("save_profile", {
      profile,
      webPassword: $("pf-pass").value || null,
      ftpPassword: $("pf-ftp-pass").value || null,
    });
    editingProfileId = dto.id;
    await loadProfiles();
    loadProfileIntoForm(dto.id);
  } catch (e) {
    err.textContent = String(e);
    err.hidden = false;
  }
}

async function deleteProfileForm() {
  if (!editingProfileId) return;
  const p = profiles.find((x) => x.id === editingProfileId);
  const ok = await confirmDialog(
    "Profil löschen?",
    `Das Profil „${p ? p.name : ""}" wird mit den gespeicherten Zugangsdaten entfernt.`,
    "Löschen",
    true,
  );
  if (!ok) return;
  try {
    await invoke("delete_profile", { id: editingProfileId });
    if (editingProfileId === activeProfileId) {
      activeProfileId = null;
      $("server-name").textContent = "— nicht verbunden —";
      setStatus(null);
      updateNavLocks();
      show("empty");
    }
    await loadProfiles();
    if (profiles.length > 0) loadProfileIntoForm(profiles[0].id);
    else newProfileForm();
  } catch (e) {
    $("profile-error").textContent = String(e);
    $("profile-error").hidden = false;
  }
}

async function connectToProfile(id) {
  try {
    const overview = await invoke("connect_profile", { id });
    activeProfileId = id;
    const p = profiles.find((x) => x.id === id) || (await invoke("list_profiles")).find((x) => x.id === id);
    $("server-name").textContent = p ? p.name : "";
    renderOverview(overview);
    updateNavLocks();
    closeManage();
    toggleServerMenu(false);
    show("overview");
    renderServerMenu();
    renderProfileList();
  } catch (e) {
    return e;
  }
  return null;
}

async function connectFromEditor() {
  const err = $("profile-error");
  err.hidden = true;
  const e = await connectToProfile(editingProfileId);
  if (e) {
    err.textContent = String(e);
    err.hidden = false;
  }
}

async function connectFromMenu(id) {
  const e = await connectToProfile(id);
  if (e) alert("Verbinden fehlgeschlagen: " + e);
}

// --- Serversteuerung im Kopf (G4, Pflichtenheft 7.7) ---
let lastOnline = null; // zuletzt bekannter Zustand (kein Hintergrund-Check, nur nach Abruf)

function setStatus(overview) {
  const badge = $("status-badge");
  if (!overview) {
    badge.textContent = "nicht verbunden";
    badge.className = "badge badge-off";
    lastOnline = null;
  } else if (overview.online) {
    badge.textContent = "läuft";
    badge.className = "badge badge-on";
    lastOnline = true;
  } else {
    badge.textContent = "gestoppt";
    badge.className = "badge badge-stopped";
    lastOnline = false;
  }
  updateControlButtons();
}

function updateControlButtons() {
  const connected = activeProfileId !== null;
  $("btn-start").hidden = !connected || lastOnline !== false;
  $("btn-restart").hidden = !connected || lastOnline !== true;
  $("btn-stop").hidden = !connected || lastOnline !== true;
}

async function refreshAfterControlAction(overview) {
  renderOverview(overview);
  if (!$("mods-view").hidden) await loadMods();
}

async function runControlAction(btn, command, busyLabel) {
  const buttons = [$("btn-start"), $("btn-restart"), $("btn-stop")];
  buttons.forEach((b) => (b.disabled = true));
  const original = btn.textContent;
  btn.textContent = busyLabel;
  try {
    const overview = await invoke(command);
    await refreshAfterControlAction(overview);
  } catch (e) {
    alert("Aktion fehlgeschlagen: " + e);
  } finally {
    buttons.forEach((b) => (b.disabled = false));
    btn.textContent = original;
  }
}

async function startServer() {
  const ok = await confirmDialog(
    "Server starten?",
    "Der Server wird gestartet. Die aktuellen Einstellungen werden dabei unverändert übernommen. Das kann einige Minuten dauern.",
    "Starten",
    false,
  );
  if (ok) await runControlAction($("btn-start"), "start_server", "Startet…");
}

async function stopServer() {
  const ok = await confirmDialog(
    "Server stoppen?",
    "Der Server wird gestoppt — verbundene Mitspieler werden getrennt.",
    "Stoppen",
    true,
  );
  if (ok) await runControlAction($("btn-stop"), "stop_server", "Stoppt…");
}

async function restartServer() {
  const ok = await confirmDialog(
    "Server neu starten?",
    "Der Server wird neu gestartet — verbundene Mitspieler werden getrennt.",
    "Neu starten",
    true,
  );
  if (ok) await runControlAction($("btn-restart"), "restart_server", "Startet neu…");
}

function renderOverview(o) {
  $("ov-state").textContent = o.online ? "Online" : "Offline";
  $("ov-version").textContent = o.version ? `Version ${o.version}` : "";
  $("ov-mods").textContent = o.mod_total;
  $("ov-mods-sub").textContent = `${o.mod_active} aktiv · ${o.mod_inactive} inaktiv`;
  $("ov-dlc").textContent = o.mod_dlc;
  setStatus(o);
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
  activeProfileId = null;
  $("server-name").textContent = "— nicht verbunden —";
  setStatus(null);
  modsState.all = [];
  modsState.selected.clear();
  updateNavLocks();
  renderServerMenu();
  renderProfileList();
  show("empty");
}

function initEyeToggle(btn) {
  btn.addEventListener("click", () => {
    const input = $(btn.dataset.eye);
    const showing = input.type === "text";
    input.type = showing ? "password" : "text";
    btn.textContent = showing ? "Anzeigen" : "Verbergen";
  });
}

window.addEventListener("DOMContentLoaded", () => {
  $("btn-refresh").addEventListener("click", refresh);
  $("btn-start").addEventListener("click", startServer);
  $("btn-restart").addEventListener("click", restartServer);
  $("btn-stop").addEventListener("click", stopServer);
  document.querySelectorAll(".nav-item").forEach((b) =>
    b.addEventListener("click", () => {
      if (!b.disabled) show(b.dataset.view);
    }),
  );
  initModsView();
  initSettingsView();

  // Statusleiste: Server-Dropdown (7.1)
  $("server-pick").addEventListener("click", (e) => {
    e.stopPropagation();
    toggleServerMenu();
  });
  document.addEventListener("click", (e) => {
    if (!$("server-wrap").contains(e.target)) toggleServerMenu(false);
  });
  $("server-menu-list").addEventListener("click", (e) => {
    const b = e.target.closest("button[data-id]");
    if (!b) return;
    toggleServerMenu(false);
    if (b.dataset.id !== activeProfileId) connectFromMenu(b.dataset.id);
  });
  $("menu-manage").addEventListener("click", () => {
    toggleServerMenu(false);
    openManage();
  });
  $("menu-disconnect").addEventListener("click", () => {
    toggleServerMenu(false);
    disconnect();
  });

  // Leerzustand → direkt in die Profilverwaltung (7.4)
  $("btn-open-manage").addEventListener("click", openManage);

  // Profilverwaltung (G1)
  $("manage-close").addEventListener("click", closeManage);
  $("btn-new-profile").addEventListener("click", newProfileForm);
  $("profile-list").addEventListener("click", (e) => {
    const c = e.target.closest(".pcard");
    if (c) loadProfileIntoForm(c.dataset.id);
  });
  $("pf-ftp").addEventListener("change", toggleFtpSection);
  $("pf-addr").addEventListener("blur", stripSchemeFromAddr);
  $("profile-form").addEventListener("submit", saveProfileForm);
  $("pf-delete").addEventListener("click", deleteProfileForm);
  $("pf-connect").addEventListener("click", connectFromEditor);
  document.querySelectorAll(".eye").forEach(initEyeToggle);

  updateNavLocks();
  show("empty");
  openManage();
});
