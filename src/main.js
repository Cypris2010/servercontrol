// Frontend ↔ Bibliothek: alle echten Operationen laufen über Tauri-Commands (src-tauri),
// die dünn auf servercontrol-core sitzen. Hier nur UI-Zustand und Aufrufe.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);

// Einheitliches Ladefeedback: Button während `fn()` sperren, Spinner zeigen (CSS
// `.is-loading::before`) und optional den Text austauschen — statt in jeder Ladefunktion
// eigene Sperr-/Text-Logik zu pflegen.
async function withBusy(button, fn, busyLabel) {
  if (!button) return fn();
  const original = button.textContent;
  button.disabled = true;
  button.classList.add("is-loading");
  if (busyLabel) button.textContent = busyLabel;
  try {
    return await fn();
  } finally {
    button.disabled = false;
    button.classList.remove("is-loading");
    if (busyLabel) button.textContent = original;
  }
}

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
      case "hub":
        return (a.from_modhub === b.from_modhub) ? 0 : a.from_modhub ? -1 : 1;
      case "issues":
        return (a.issue_count || 0) - (b.issue_count || 0);
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
        <td class="c-ver">${m.version || "—"}${m.update_available ? ' <span class="update-badge" title="Update im ModHub verfügbar">↑</span>' : ""}</td>
        <td>${author}</td>
        <td class="c-file">${m.file_name}</td>
        <td class="c-size">${fmtSize(m.size)}</td>
        <td class="c-dlc">${m.is_dlc ? '<span class="dlc-tag">DLC</span>' : "—"}</td>
        <td class="c-hub">${m.from_modhub ? '<span class="hub-tag" title="Von ModHub installiert">ModHub</span>' : '<span class="hub-tag hub-tag-local" title="Nicht von ModHub, z. B. manuell hochgeladen">Lokal</span>'}</td>
        <td class="c-issues">${
          m.issue_count > 0
            ? `<span class="issues-tag" title="${escapeHtml((m.issues || []).join("\n")) || "Details werden geladen…"}">${m.issue_count}</span>`
            : "—"
        }</td>
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
    const view = await withBusy($("btn-mods-refresh"), () => invoke("mods_view"));
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
    settingsView = await withBusy($("btn-settings-refresh"), () => invoke("settings_view"));
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

// --- Bereitstellen (G3, Pflichtenheft 7.6) — vorerst nur Datei-Upload ---
// Ablauf: Datei(en)/Ordner wählen → Prüfschritt (plan_uploads, kein Upload) → pro Datei eine
// Entscheidung (Hochladen/Überschreiben/Nicht hochladen) → Start läuft die nicht übersprungenen
// nacheinander (ein Fortschritts-Event-Kanal), mit eigenem Balken je Zeile.
let uploadPlan = []; // { path, file_name, is_fs25_mod, local_version, exists_on_server, server_version, server_status, decision }
let uploadRunning = false; // true ab Klick auf „Start" — steuert, ob „×" aus dem Plan nimmt oder nur die Zeile ausblendet

function basename(p) {
  return p.split(/[\\/]/).pop();
}

function fmtBytes(n) {
  const mb = n / (1024 * 1024);
  return mb >= 1000 ? (mb / 1024).toFixed(2) + " GB" : mb.toFixed(1) + " MB";
}

function defaultDecision(item) {
  if (!item.is_fs25_mod) return "skip";
  return item.exists_on_server ? "skip" : "upload";
}

async function buildUploadPlan(paths) {
  const err = $("upload-error");
  err.hidden = true;
  uploadRunning = false;
  $("upload-file-name").textContent = `Prüfe ${paths.length === 1 ? "1 Datei" : paths.length + " Dateien"}…`;
  try {
    const items = await invoke("plan_uploads", { paths });
    uploadPlan = items.map((it) => ({ ...it, decision: defaultDecision(it) }));
    $("upload-file-name").textContent =
      paths.length === 1 ? basename(paths[0]) : `${paths.length} Dateien: ${paths.map(basename).join(", ")}`;
    renderUploadPlan();
  } catch (e) {
    err.textContent = "Prüfen fehlgeschlagen: " + e;
    err.hidden = false;
  }
}

async function pickModFile() {
  const err = $("upload-error");
  err.hidden = true;
  try {
    const path = await invoke("plugin:dialog|open", {
      options: {
        multiple: true,
        filters: [{ name: "Mod", extensions: ["zip", "dlc"] }],
      },
    });
    if (!path) return;
    await buildUploadPlan(Array.isArray(path) ? path : [path]);
  } catch (e) {
    err.textContent = "Dateiauswahl fehlgeschlagen: " + e;
    err.hidden = false;
  }
}

async function pickModFolder() {
  const err = $("upload-error");
  err.hidden = true;
  try {
    const dir = await invoke("plugin:dialog|open", {
      options: { directory: true, multiple: false },
    });
    if (!dir) return;
    const dirPath = Array.isArray(dir) ? dir[0] : dir;
    $("upload-file-name").textContent = "Durchsuche Ordner…";
    const paths = await invoke("list_mod_files", { dir: dirPath });
    if (paths.length === 0) {
      $("upload-file-name").textContent = "keine Datei gewählt";
      err.textContent = "Keine FS25-Mods (.zip/.dlc mit modDesc.xml) in diesem Ordner gefunden.";
      err.hidden = false;
      return;
    }
    await buildUploadPlan(paths);
  } catch (e) {
    err.textContent = "Ordnerauswahl fehlgeschlagen: " + e;
    err.hidden = false;
  }
}

function decisionOptionsHtml(item) {
  const overwriteDisabled = lastOnline ? "disabled" : "";
  if (item.exists_on_server) {
    return `<option value="skip">Nicht hochladen</option>
      <option value="overwrite" ${overwriteDisabled}>Überschreiben</option>`;
  }
  return `<option value="upload">Hochladen</option>
    <option value="skip">Nicht hochladen</option>`;
}

function renderUploadPlan() {
  $("upload-plan-controls").hidden = uploadPlan.length === 0;
  $("upload-queue").innerHTML = uploadPlan
    .map((item, i) => {
      const badge = !item.is_fs25_mod
        ? '<span class="badge badge-orphan">Kein FS25-Mod</span>'
        : item.exists_on_server
          ? '<span class="badge badge-inactive">Vorhanden</span>'
          : '<span class="badge badge-active">Neu</span>';
      let versions;
      if (!item.is_fs25_mod) {
        versions = "Keine <code>modDesc.xml</code> gefunden — vermutlich kein FS25-Mod.";
      } else if (item.exists_on_server) {
        versions = `Eigene Version: <b>${escapeHtml(item.local_version || "unbekannt")}</b> ·
           Server-Version: <b>${escapeHtml(item.server_version || "unbekannt")}</b>
           (${STATUS_LABEL[item.server_status] || item.server_status})`;
      } else {
        versions = `Version: <b>${escapeHtml(item.local_version || "unbekannt")}</b>`;
      }
      return `<div class="upload-row" id="upload-row-${i}">
        <button type="button" class="upload-row-close" data-idx="${i}" title="Entfernen">×</button>
        <div class="upload-row-head">
          <span class="upload-row-name">${escapeHtml(item.file_name)}</span>
          ${badge}
        </div>
        <div class="upload-row-versions">${versions}</div>
        <div class="upload-row-decision" id="upload-row-decision-${i}">
          <select class="inp" data-idx="${i}">${decisionOptionsHtml(item)}</select>
        </div>
        <div class="progress-wrap" id="upload-row-fill-wrap-${i}" hidden>
          <div class="progress-fill" id="upload-row-fill-${i}"></div>
        </div>
        <div class="upload-row-status" id="upload-row-status-${i}"></div>
      </div>`;
    })
    .join("");
  uploadPlan.forEach((item, i) => {
    $(`upload-row-${i}`).querySelector("select").value = item.decision;
  });
}

function setAllDecisions(decision) {
  uploadPlan.forEach((item) => {
    if (decision === "skip" || !item.is_fs25_mod) {
      item.decision = "skip";
    } else if (item.exists_on_server) {
      item.decision = lastOnline ? "skip" : "overwrite";
    } else {
      item.decision = "upload";
    }
  });
  renderUploadPlan();
}

// Vergleicht zwei punktgetrennte Versionsnummern (z. B. "1.0.0.1") Segment für Segment
// numerisch. > 0 wenn a neuer, < 0 wenn a älter, 0 wenn gleich, null wenn nicht vergleichbar.
function compareVersions(a, b) {
  const pa = String(a).split(".").map((n) => parseInt(n, 10));
  const pb = String(b).split(".").map((n) => parseInt(n, 10));
  const len = Math.max(pa.length, pb.length);
  for (let i = 0; i < len; i++) {
    const na = pa[i] || 0;
    const nb = pb[i] || 0;
    if (Number.isNaN(na) || Number.isNaN(nb)) return null;
    if (na !== nb) return na - nb;
  }
  return 0;
}

// Nur bereits vorhandene Mods mit einer eigenen Version, die höher ist als die auf dem Server,
// auf „Überschreiben" stellen — alle anderen vorhandenen auf „Nicht hochladen". Neue Mods
// (noch nicht auf dem Server) bleiben unverändert.
function setNewerOnly() {
  uploadPlan.forEach((item) => {
    if (!item.is_fs25_mod || !item.exists_on_server) return;
    if (lastOnline) {
      item.decision = "skip";
      return;
    }
    const cmp =
      item.local_version && item.server_version
        ? compareVersions(item.local_version, item.server_version)
        : null;
    item.decision = cmp !== null && cmp > 0 ? "overwrite" : "skip";
  });
  renderUploadPlan();
}

function removeUploadRow(idx) {
  if (uploadRunning) {
    $(`upload-row-${idx}`)?.remove();
    return;
  }
  uploadPlan.splice(idx, 1);
  renderUploadPlan();
}

async function startUploadPlan() {
  const toRun = uploadPlan.filter((it) => it.decision !== "skip");
  const err = $("upload-error");
  err.hidden = true;
  if (toRun.some((it) => it.decision === "overwrite")) {
    const ok = await confirmDialog(
      "Mods überschreiben?",
      "Die ausgewählten, bereits vorhandenen Mod-Dateien werden vor dem Neu-Hochladen vom Server gelöscht — das lässt sich nicht rückgängig machen.",
      "Überschreiben",
      true,
    );
    if (!ok) return;
  }

  uploadRunning = true;
  $("upload-plan-controls").hidden = true;
  const failedNames = [];

  for (let i = 0; i < uploadPlan.length; i++) {
    const item = uploadPlan[i];
    const row = $(`upload-row-${i}`);
    if (!row) continue; // per „×" vor dem Start entfernt
    $(`upload-row-decision-${i}`).hidden = true;
    const fillWrap = $(`upload-row-fill-wrap-${i}`);
    const fill = $(`upload-row-fill-${i}`);
    const statusEl = $(`upload-row-status-${i}`);

    if (item.decision === "skip") {
      statusEl.textContent = "übersprungen";
      continue;
    }
    fillWrap.hidden = false;
    statusEl.textContent = item.decision === "overwrite" ? "löscht vorhandene Version…" : "lädt hoch…";

    const unlisten = await listen("progress", (event) => {
      const { done, total } = event.payload;
      statusEl.textContent = total
        ? `${fmtBytes(done)} von ${fmtBytes(total)} (${Math.round((done / total) * 100)} %)`
        : `${fmtBytes(done)} hochgeladen…`;
      if (total) fill.style.width = `${Math.min(100, (done / total) * 100)}%`;
    });
    try {
      if (item.decision === "overwrite") {
        await invoke("overwrite_mod", { path: item.path, fileName: item.file_name });
      } else {
        await invoke("upload_mod", { path: item.path });
      }
      fill.style.width = "100%";
      statusEl.textContent = "fertig";
      statusEl.classList.add("done");
    } catch (e) {
      fill.classList.add("failed");
      statusEl.textContent = "fehlgeschlagen: " + e;
      statusEl.classList.add("failed");
      failedNames.push(item.file_name);
    } finally {
      unlisten();
    }
  }

  uploadPlan = [];
  $("upload-file-name").textContent = "keine Datei gewählt";
  if (failedNames.length > 0) {
    err.textContent = "Fehlgeschlagen: " + failedNames.join(", ");
    err.hidden = false;
  }
}

function initDeployView() {
  $("btn-pick-file").addEventListener("click", pickModFile);
  $("btn-pick-folder").addEventListener("click", pickModFolder);
  $("btn-bulk-go").addEventListener("click", () => setAllDecisions("go"));
  $("btn-bulk-skip").addEventListener("click", () => setAllDecisions("skip"));
  $("btn-bulk-newer").addEventListener("click", setNewerOnly);
  $("btn-start-upload").addEventListener("click", startUploadPlan);
  $("upload-queue").addEventListener("change", (e) => {
    const select = e.target.closest("select[data-idx]");
    if (!select) return;
    uploadPlan[Number(select.dataset.idx)].decision = select.value;
  });
  $("upload-queue").addEventListener("click", (e) => {
    const btn = e.target.closest(".upload-row-close");
    if (!btn) return;
    removeUploadRow(Number(btn.dataset.idx));
  });

  $("tab-upload").addEventListener("click", () => showDeployTab("upload"));
  $("tab-modhub").addEventListener("click", () => showDeployTab("modhub"));
  showDeployTab("upload");

  $("btn-modhub-search").addEventListener("click", runModhubSearch);
  $("modhub-query").addEventListener("keydown", (e) => {
    if (e.key === "Enter") runModhubSearch();
  });
  $("btn-modhub-browse").addEventListener("click", runModhubBrowse);
  $("modhub-results").addEventListener("click", (e) => {
    const btn = e.target.closest(".modhub-install-btn");
    if (!btn) return;
    const modId = Number(btn.dataset.modId);
    installModhubMod(modId, modhubFileNames[modId]);
  });
}

// --- ModHub-Suche (G3b, Kann-Ziel, Pflichtenheft 4.4 / 7.7 LH) ---

function showDeployTab(tab) {
  $("tab-upload").classList.toggle("active", tab === "upload");
  $("tab-upload").setAttribute("aria-selected", tab === "upload");
  $("tab-modhub").classList.toggle("active", tab === "modhub");
  $("tab-modhub").setAttribute("aria-selected", tab === "modhub");
  $("deploy-panel-upload").hidden = tab !== "upload";
  $("deploy-panel-modhub").hidden = tab !== "modhub";
}

// mod_id -> Dateiname, wenn schon bekannt (aus der Server-Kategorieseite) — dann kann
// `modhub_install` sich den zusätzlichen Abruf der öffentlichen ModHub-Detailseite sparen.
let modhubFileNames = {};

function modhubResultHtml(entry, installedVersion) {
  const stars = entry.rating != null ? `★ ${entry.rating.toFixed(1)}` : "";
  const versionText =
    installedVersion !== undefined
      ? `Installiert: ${installedVersion || "unbekannt"} → Neu: ${entry.version || "unbekannt"}`
      : entry.version
        ? "v" + entry.version
        : "";
  const meta = [entry.author, stars, versionText, entry.size ? fmtBytes(entry.size) : ""]
    .filter(Boolean)
    .join(" · ");
  const installDisabled = lastOnline !== false ? "disabled" : "";
  return `<div class="modhub-card" id="modhub-card-${entry.mod_id}">
    <div class="modhub-card-body">
      <div class="modhub-card-name">${escapeHtml(entry.name)}</div>
      <div class="modhub-card-meta">${escapeHtml(meta)}</div>
      <div class="progress-wrap" id="modhub-fill-wrap-${entry.mod_id}" hidden>
        <div class="progress-fill" id="modhub-fill-${entry.mod_id}"></div>
      </div>
      <div class="modhub-card-status" id="modhub-status-${entry.mod_id}"></div>
    </div>
    <button type="button" class="ghost small modhub-install-btn" data-mod-id="${entry.mod_id}" ${installDisabled}>
      Auf Server installieren
    </button>
  </div>`;
}

async function runModhubSearch() {
  const query = $("modhub-query").value.trim();
  const err = $("modhub-error");
  err.hidden = true;
  if (!query) return;
  modhubFileNames = {};
  $("modhub-results").innerHTML = `<p class="hint-inline">Suche…</p>`;
  try {
    const entries = await invoke("modhub_search", { query });
    if (entries.length === 0) {
      $("modhub-results").innerHTML = `<p class="hint-inline">Keine Treffer.</p>`;
      return;
    }
    $("modhub-results").innerHTML = entries.map(modhubResultHtml).join("");
  } catch (e) {
    $("modhub-results").innerHTML = "";
    err.textContent = "Suche fehlgeschlagen: " + e;
    err.hidden = false;
  }
}

// Kategorie „Update" (3): dort ist praktisch jeder Treffer bereits installiert, ein Abgleich
// mit der eigenen Version lohnt sich also. Bei den anderen Kategorien (meist nicht installiert)
// wäre der zusätzliche Abruf der Modliste nur Rauschen — deshalb nur hier.
const MODHUB_UPDATE_CATEGORY = 3;

async function runModhubBrowse() {
  const category = Number($("modhub-category").value);
  const err = $("modhub-error");
  err.hidden = true;
  modhubFileNames = {};
  $("modhub-results").innerHTML = `<p class="hint-inline">Lade Kategorie…</p>`;
  try {
    const entries = await invoke("modhub_browse_category", { category, page: 0 });
    entries.forEach((e) => (modhubFileNames[e.mod_id] = e.file_name));
    if (entries.length === 0) {
      $("modhub-results").innerHTML = `<p class="hint-inline">Keine Einträge in dieser Kategorie.</p>`;
      return;
    }
    let installedByFile = null;
    if (category === MODHUB_UPDATE_CATEGORY) {
      const modsView = await invoke("mods_view");
      installedByFile = {};
      modsView.mods.forEach((m) => (installedByFile[m.file_name] = m.version));
    }
    $("modhub-results").innerHTML = entries
      .map((e) => modhubResultHtml(e, installedByFile ? installedByFile[e.file_name] : undefined))
      .join("");
  } catch (e) {
    $("modhub-results").innerHTML = "";
    err.textContent = "Kategorie laden fehlgeschlagen: " + e;
    err.hidden = false;
  }
}

async function installModhubMod(modId, fileName) {
  if (lastOnline !== false) return;
  const err = $("modhub-error");
  err.hidden = true;
  const btn = $(`modhub-card-${modId}`)?.querySelector(".modhub-install-btn");
  const fillWrap = $(`modhub-fill-wrap-${modId}`);
  const fill = $(`modhub-fill-${modId}`);
  const statusEl = $(`modhub-status-${modId}`);
  if (btn) btn.disabled = true;
  if (fillWrap) fillWrap.hidden = false;
  if (statusEl) statusEl.textContent = "wird gestartet…";

  const unlisten = await listen("modhub-progress", (event) => {
    const { mod_id, done, total } = event.payload;
    if (mod_id !== modId) return;
    if (statusEl) {
      statusEl.textContent = total
        ? `${fmtBytes(done)} von ${fmtBytes(total)} (${Math.round((done / total) * 100)} %)`
        : `${fmtBytes(done)} geladen…`;
    }
    if (fill && total) fill.style.width = `${Math.min(100, (done / total) * 100)}%`;
  });
  try {
    await invoke("modhub_install", { modId, fileName: fileName || null });
    if (fill) fill.style.width = "100%";
    if (statusEl) {
      statusEl.textContent = "installiert";
      statusEl.classList.add("done");
    }
  } catch (e) {
    if (fill) fill.classList.add("failed");
    if (statusEl) {
      statusEl.textContent = "fehlgeschlagen: " + e;
      statusEl.classList.add("failed");
    }
    if (btn) btn.disabled = false;
  } finally {
    unlisten();
  }
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
  $("deploy-view").hidden = view !== "deploy";
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

async function connectToProfile(id, button) {
  // Menü/Editor schließen meist sofort — der Statusbadge im Kopf bleibt aber immer sichtbar,
  // daher zeigt der während des Logins den Ladezustand (zusätzlich zum Button, falls der noch
  // im Bild ist, z. B. im Editor).
  const badge = $("status-badge");
  const badgeBefore = { text: badge.textContent, cls: badge.className };
  badge.textContent = "verbinde…";
  badge.className = "badge badge-off is-loading";
  try {
    const overview = await withBusy(button, () => invoke("connect_profile", { id }), "Verbinden…");
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
    badge.textContent = badgeBefore.text;
    badge.className = badgeBefore.cls;
    return e;
  }
  return null;
}

async function connectFromEditor() {
  const err = $("profile-error");
  err.hidden = true;
  const e = await connectToProfile(editingProfileId, $("pf-connect"));
  if (e) {
    err.textContent = String(e);
    err.hidden = false;
  }
}

async function connectFromMenu(id, button) {
  const e = await connectToProfile(id, button);
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
  document.querySelectorAll(".modhub-install-btn").forEach((btn) => {
    btn.disabled = lastOnline !== false;
  });
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
  try {
    renderOverview(await withBusy($("btn-refresh"), () => invoke("overview")));
  } catch (e) {
    alert("Aktualisieren fehlgeschlagen: " + e);
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
  initDeployView();

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
    if (b.dataset.id !== activeProfileId) connectFromMenu(b.dataset.id, b);
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
