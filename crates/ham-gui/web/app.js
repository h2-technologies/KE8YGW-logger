const state = {
  shell: null,
  commands: [],
  plugins: [],
  runtimeEvents: [],
  runtimeStatus: null,
  qsos: [],
  qsoError: "",
  importSummary: null,
  syncState: null,
  selectedPeerId: null,
  activeWorkspace: "dashboard",
  busConnected: false,
  streamPaused: false,
  selectedEventId: null,
  monitorFilters: {
    severity: "",
    category: "",
    source: "",
    text: "",
  },
};

const byId = (id) => document.getElementById(id);

async function boot() {
  const payload = await fetch("/api/shell").then((response) => response.json());
  state.shell = payload.shell;
  state.commands = payload.commands.commands;
  state.plugins = payload.plugins;
  state.runtimeEvents = payload.runtime_events;
  state.runtimeStatus = payload.runtime_status;
  state.activeWorkspace = payload.shell.active_workspace;
  state.busConnected = payload.runtime_status.connected;

  bindShellControls();
  renderWorkspaceSelector();
  await refreshQsos();
  await refreshSyncState();
  render();
  startRuntimeEventPolling();
}

function bindShellControls() {
  document.querySelectorAll(".activity-item").forEach((button) => {
    button.addEventListener("click", () => switchWorkspace(button.dataset.workspace));
  });
  byId("workspace-selector").addEventListener("change", (event) => switchWorkspace(event.target.value));
  byId("command-button").addEventListener("click", openCommandPalette);
  byId("import-adif-button").addEventListener("click", importAdifFromPrompt);
  byId("export-adif-button").addEventListener("click", exportAdifFromPrompt);
  byId("settings-button").addEventListener("click", () => openScreen("settings"));
  byId("plugins-button").addEventListener("click", () => openScreen("plugins"));
  byId("close-screen").addEventListener("click", closeScreen);
  byId("command-search").addEventListener("input", renderCommandResults);

  document.addEventListener("keydown", (event) => {
    const key = event.key.toLowerCase();
    if ((event.ctrlKey || event.metaKey) && key === "k") {
      event.preventDefault();
      openCommandPalette();
    }
    if (event.key === "Escape") {
      closeScreen();
      closeCommandPalette();
    }
  });
}

function renderWorkspaceSelector() {
  const selector = byId("workspace-selector");
  selector.innerHTML = state.shell.workspaces
    .map((workspace) => `<option value="${workspace.id}">${workspace.title}</option>`)
    .join("");
}

function render() {
  const workspace = currentWorkspace();
  byId("workspace-title").textContent = workspace.title;
  byId("workspace-selector").value = state.activeWorkspace;
  byId("status-workspace").textContent = `Workspace: ${workspace.title}`;
  byId("status-plugins").textContent = `Plugins: ${state.plugins.filter((plugin) => plugin.enabled).length} enabled`;
  byId("status-bus").textContent = `Event bus: ${state.busConnected ? "connected" : "disconnected"}`;
  byId("status-sync").textContent = `Sync: ${state.runtimeStatus?.sync_state || "Local only"}`;
  byId("status-events").textContent = `Runtime events: ${state.runtimeStatus?.runtime_event_count || state.runtimeEvents.length}`;
  byId("status-errors").textContent = `Errors: ${state.runtimeStatus?.latest_error_count || 0}`;
  byId("status-sync-peers").textContent = `Discovery: ${state.syncState?.discovery_running ? "running" : "stopped"} / ${state.syncState?.peers?.length || 0} peers`;

  document.querySelectorAll(".activity-item").forEach((button) => {
    button.classList.toggle("is-active", button.dataset.workspace === state.activeWorkspace);
  });

  renderRegion("center-panels", "center");
  renderRegion("right-panels", "right-inspector");
  renderRegion("bottom-panels", "bottom");
  bindPanelControls();
}

function renderRegion(elementId, region) {
  const workspace = currentWorkspace();
  const placements = workspace.layout.placements
    .filter((placement) => placement.region === region)
    .sort((left, right) => left.order - right.order);
  byId(elementId).innerHTML = placements.map((placement) => renderPanel(placement.panel_id)).join("");
}

function renderPanel(panelId) {
  const panel = state.shell.panels.find((candidate) => candidate.id === panelId);
  if (!panel) return "";
  return `
    <article class="panel-card" data-panel-id="${panel.id}">
      <header>
        <h3>${panel.title}</h3>
        <span class="source">${panel.source}</span>
      </header>
      <div class="panel-content">${panelContent(panel)}</div>
    </article>
  `;
}

function panelContent(panel) {
  switch (panel.id) {
    case "recent-qsos":
      return renderRecentQsos();
    case "callsign-entry":
      return renderCallsignEntry();
    case "event-bus-monitor":
      return renderEventBusMonitor();
    case "plugin-permissions":
      return state.plugins
        .map((plugin) => `<p><strong>${plugin.name}</strong><br />${plugin.requested_permissions.map((permission) => `<span class="pill">${permission}</span>`).join("")}</p>`)
        .join("");
    case "sync-status":
      return renderSyncStatus();
    case "rig-control":
      return `<p>Rig control plugin surface placeholder.</p><button class="toolbar-button" disabled>Connect Rig</button>`;
    case "map-placeholder":
      return `<p>Map and activation geography placeholder. Future plugin should provide map tiles and overlays.</p>`;
    case "pota-sota-activation":
      return `<p>Activation reference, operator profile, and portable log context will appear here.</p>`;
    case "dx-cluster":
      return `<p>DX cluster feed placeholder. Network integrations stay plugin-owned.</p>`;
    case "ai-assistant":
      return `<p>AI assistant placeholder. Future access should be permissioned and proposal-aware.</p>`;
    case "diagnostic-reports":
      return `<p>Diagnostic report placeholder for event bus, store, sync, and plugin runtime health.</p>`;
    default:
      return `<p>${panel.title} placeholder panel. Required permissions: ${panel.required_permissions.join(", ") || "none"}.</p>`;
  }
}

function renderRuntimeEvent(event) {
  const selected = state.selectedEventId === event.event_id ? " is-selected" : "";
  return `<button class="event-row${selected}" type="button" data-event-id="${event.event_id}">
    <span class="event-main">
      <strong>${event.event_type}</strong>
      <span>${event.payload_summary}</span>
      ${event.error ? `<span class="event-error">${event.error}</span>` : ""}
    </span>
    <span class="event-meta">
      <span class="severity severity-${event.severity}">${event.severity}</span>
      <small>${event.source}</small>
      <small>${new Date(event.timestamp).toLocaleTimeString()}</small>
    </span>
  </button>`;
}

function switchWorkspace(workspaceId) {
  state.activeWorkspace = workspaceId;
  render();
}

function currentWorkspace() {
  return state.shell.workspaces.find((workspace) => workspace.id === state.activeWorkspace) || state.shell.workspaces[0];
}

function openCommandPalette() {
  const dialog = byId("command-palette");
  byId("command-search").value = "";
  renderCommandResults();
  dialog.showModal();
  byId("command-search").focus();
}

function closeCommandPalette() {
  const dialog = byId("command-palette");
  if (dialog.open) dialog.close();
}

function renderCommandResults() {
  const query = byId("command-search").value.toLowerCase();
  const commands = state.commands.filter((command) =>
    `${command.id} ${command.title} ${command.category}`.toLowerCase().includes(query),
  );
  byId("command-results").innerHTML = commands.map(renderCommand).join("");
  document.querySelectorAll(".command-row").forEach((row) => {
    row.addEventListener("click", () => runCommand(row.dataset.commandId));
  });
}

function renderCommand(command) {
  return `<div class="command-row" role="option" data-command-id="${command.id}">
    <span>${command.title}<br /><small>${command.category}</small></span>
    <small>${command.shortcut || ""}</small>
  </div>`;
}

function runCommand(commandId) {
  const command = state.commands.find((candidate) => candidate.id === commandId);
  if (!command) return;

  if (command.target_workspace) switchWorkspace(command.target_workspace);
  if (command.id === "open.settings") openScreen("settings");
  if (command.id === "open.plugins") openScreen("plugins");
  if (command.id === "open.diagnostics") openScreen("diagnostics");
  if (command.id === "diagnostics.open-folder") openScreen("diagnostics-folder");
  if (command.id === "adif.import") importAdifFromPrompt();
  if (command.id === "adif.export") exportAdifFromPrompt();
  if (command.id === "official-log.verify-chain") verifyLogChain();
  if (command.id === "projection.rebuild") rebuildProjections();
  if (command.id === "sync.discovery.start") startDiscovery();
  if (command.id === "sync.discovery.stop") stopDiscovery();
  if (command.id === "sync.peers.refresh") refreshPeers();
  if (command.id === "sync.handshake.selected") handshakeSelectedPeer();
  if (command.id === "sync.identity.copy") copyLocalSyncIdentity();
  if (command.id === "event-bus.open") switchWorkspace("dashboard");
  if (command.id === "event-bus.pause") toggleRuntimeStream();
  if (command.id === "event-bus.export") exportVisibleRuntimeEvents();
  if (command.id === "event-bus.copy-latest-error") copyLatestError();
  if (command.id === "focus.callsign-entry") {
    switchWorkspace("casual-logger");
    requestAnimationFrame(() => byId("callsign-entry-input")?.focus());
  }
  if (command.id === "toggle.event-bus-monitor") switchWorkspace("dashboard");
  closeCommandPalette();
}

function openScreen(kind) {
  const overlay = byId("overlay");
  const title = byId("screen-title");
  const eyebrow = byId("screen-eyebrow");
  const body = byId("screen-body");
  overlay.hidden = false;

  if (kind === "plugins") {
    eyebrow.textContent = "Plugin Runtime";
    title.textContent = "Plugin Manager";
    body.innerHTML = renderPluginManager();
    return;
  }

  if (kind === "diagnostics") {
    eyebrow.textContent = "Diagnostics";
    title.textContent = "Diagnostic Report";
    body.innerHTML = `<p class="muted">Future report export will include core event store, sync, plugin runtime, and GUI bridge state.</p>
      <div class="stack">${state.runtimeEvents.map(renderRuntimeEvent).join("")}</div>`;
    return;
  }

  if (kind === "diagnostics-folder") {
    eyebrow.textContent = "Diagnostics";
    title.textContent = "Diagnostics Folder";
    body.innerHTML = `<p class="muted">Runtime JSONL logs are written here. Opening the folder through the OS shell is not wired yet.</p>
      <pre class="path-block">${state.runtimeStatus?.log_directory || "unknown"}</pre>`;
    return;
  }

  if (kind === "import-summary") {
    eyebrow.textContent = "ADIF";
    title.textContent = "Import Summary";
    body.innerHTML = `<pre class="path-block">${JSON.stringify(state.importSummary, null, 2)}</pre>`;
    return;
  }

  eyebrow.textContent = "Application";
  title.textContent = "Settings";
  body.innerHTML = renderSettings();
}

function closeScreen() {
  byId("overlay").hidden = true;
}

function renderPluginManager() {
  return `<p class="muted">Enable and disable controls are placeholders until real plugin loading is implemented.</p>
    <div class="plugin-grid">
      ${state.plugins
        .map(
          (plugin) => `<article class="plugin-card">
            <h3>${plugin.name}</h3>
            <p class="muted">${plugin.plugin_id}</p>
            <p>${plugin.requested_permissions.map((permission) => `<span class="pill">${permission}</span>`).join("")}</p>
            <button class="toolbar-button" disabled>${plugin.enabled ? "Enabled" : "Disabled"}</button>
          </article>`,
        )
        .join("")}
    </div>`;
}

function renderSettings() {
  const sections = ["General", "Appearance", "Callsign/Profile", "Sync", "Plugins", "Diagnostics", "Keyboard Shortcuts"];
  return `<div class="settings-grid">
    ${sections
      .map((section) => `<article class="settings-card"><h3>${section}</h3><p class="muted">Placeholder settings surface.</p></article>`)
      .join("")}
  </div>`;
}

function renderEventBusMonitor() {
  return `<div class="monitor-controls">
      <label>Severity
        <select id="monitor-severity" class="placeholder-control" aria-label="Filter runtime events by severity">
          ${option("", "All", state.monitorFilters.severity)}
          ${["trace", "debug", "info", "warn", "error"].map((value) => option(value, value, state.monitorFilters.severity)).join("")}
        </select>
      </label>
      <label>Category
        <select id="monitor-category" class="placeholder-control" aria-label="Filter runtime events by category">
          ${option("", "All", state.monitorFilters.category)}
          ${["ui", "plugin", "sync", "rig", "network", "proposal", "projection", "diagnostics", "app"]
            .map((value) => option(value, value, state.monitorFilters.category))
            .join("")}
        </select>
      </label>
      <label>Source
        <input id="monitor-source" class="placeholder-control" aria-label="Filter runtime events by source" value="${state.monitorFilters.source}" />
      </label>
      <label>Search
        <input id="monitor-text" class="placeholder-control" aria-label="Search runtime events" value="${state.monitorFilters.text}" />
      </label>
    </div>
    <div class="monitor-actions">
      <button id="monitor-pause" class="toolbar-button" type="button">${state.streamPaused ? "Resume" : "Pause"}</button>
      <button id="monitor-clear" class="toolbar-button" type="button">Clear View</button>
      <button id="monitor-copy" class="toolbar-button" type="button">Copy Selected</button>
      <button id="monitor-export" class="toolbar-button" type="button">Export JSONL</button>
    </div>
    <div class="event-list">${state.runtimeEvents.map(renderRuntimeEvent).join("") || `<p class="muted">No runtime events match the current filters.</p>`}</div>`;
}

function option(value, label, selectedValue) {
  return `<option value="${value}" ${value === selectedValue ? "selected" : ""}>${label}</option>`;
}

function bindPanelControls() {
  document.querySelectorAll(".event-row").forEach((row) => {
    row.addEventListener("click", () => {
      state.selectedEventId = row.dataset.eventId;
      render();
    });
  });

  const severity = byId("monitor-severity");
  if (severity) {
    severity.addEventListener("change", (event) => updateMonitorFilter("severity", event.target.value));
    byId("monitor-category").addEventListener("change", (event) => updateMonitorFilter("category", event.target.value));
    byId("monitor-source").addEventListener("change", (event) => updateMonitorFilter("source", event.target.value));
    byId("monitor-text").addEventListener("change", (event) => updateMonitorFilter("text", event.target.value));
    byId("monitor-pause").addEventListener("click", toggleRuntimeStream);
    byId("monitor-clear").addEventListener("click", () => {
      state.runtimeEvents = [];
      state.selectedEventId = null;
      render();
    });
    byId("monitor-copy").addEventListener("click", copySelectedRuntimeEvent);
    byId("monitor-export").addEventListener("click", exportVisibleRuntimeEvents);
  }

  const qsoForm = byId("qso-create-form");
  if (qsoForm) {
    qsoForm.addEventListener("submit", submitQsoCreate);
  }

  document.querySelectorAll("[data-qso-action]").forEach((button) => {
    button.addEventListener("click", () => {
      runQsoAction(button.dataset.qsoAction, button.dataset.qsoId);
    });
  });

  document.querySelectorAll("[data-peer-id]").forEach((button) => {
    button.addEventListener("click", () => {
      state.selectedPeerId = button.dataset.peerId;
      render();
    });
  });
  const start = byId("sync-start-discovery");
  if (start) {
    start.addEventListener("click", startDiscovery);
    byId("sync-stop-discovery").addEventListener("click", stopDiscovery);
    byId("sync-refresh-peers").addEventListener("click", refreshPeers);
    byId("sync-handshake").addEventListener("click", handshakeSelectedPeer);
    byId("sync-copy-identity").addEventListener("click", copyLocalSyncIdentity);
  }
}

function updateMonitorFilter(key, value) {
  state.monitorFilters[key] = value;
  refreshRuntimeEvents();
}

function toggleRuntimeStream() {
  state.streamPaused = !state.streamPaused;
  render();
}

async function refreshRuntimeEvents() {
  if (state.streamPaused) return;
  const payload = await fetch(`/api/runtime-events?${runtimeQuery()}`).then((response) => response.json());
  state.runtimeEvents = payload.runtime_events;
  state.runtimeStatus = payload.runtime_status;
  state.busConnected = payload.runtime_status.connected;
  render();
}

function startRuntimeEventPolling() {
  refreshRuntimeEvents();
  setInterval(refreshRuntimeEvents, 2000);
}

function runtimeQuery() {
  const params = new URLSearchParams();
  Object.entries(state.monitorFilters).forEach(([key, value]) => {
    if (value) params.set(key, value);
  });
  return params.toString();
}

function copySelectedRuntimeEvent() {
  const event = state.runtimeEvents.find((candidate) => candidate.event_id === state.selectedEventId) || state.runtimeEvents[0];
  if (!event) return;
  navigator.clipboard?.writeText(JSON.stringify(event, null, 2));
}

function copyLatestError() {
  const event = state.runtimeEvents.find((candidate) => candidate.severity === "error" || candidate.error);
  if (event) navigator.clipboard?.writeText(JSON.stringify(event, null, 2));
}

async function exportVisibleRuntimeEvents() {
  const response = await fetch(`/api/runtime-events/export?${runtimeQuery()}`);
  const blob = await response.blob();
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "runtime-events.jsonl";
  link.click();
  URL.revokeObjectURL(url);
}

function renderCallsignEntry() {
  return `<form id="qso-create-form" class="qso-form">
      <label>Contacted callsign
        <input id="callsign-entry-input" name="contacted_callsign" class="placeholder-control" aria-label="Contacted callsign" placeholder="K1ABC" required />
      </label>
      <label>Mode
        <input name="mode" class="placeholder-control" aria-label="Mode" placeholder="SSB" required />
      </label>
      <label>Frequency Hz
        <input name="frequency_hz" class="placeholder-control" aria-label="Frequency Hz" placeholder="14250000" inputmode="numeric" />
      </label>
      <label>Band
        <input name="band" class="placeholder-control" aria-label="Band" placeholder="20m" />
      </label>
      <label>Notes
        <input name="notes" class="placeholder-control" aria-label="Notes" placeholder="Optional note" />
      </label>
      <button class="toolbar-button" type="submit">Submit QSO Proposal</button>
      ${state.qsoError ? `<p class="event-error">${state.qsoError}</p>` : ""}
    </form>
    <p>Submits a proposal to ham-core; the GUI does not write official events directly.</p>`;
}

function renderRecentQsos() {
  if (!state.qsos.length) {
    return `<p class="muted">No visible QSOs yet. Create one from Callsign Entry.</p>`;
  }
  return `<div class="qso-list">
    ${state.qsos
      .map((qso) => {
        const payload = qso.payload;
        return `<article class="qso-row">
          <strong>${payload.contacted_callsign || "Unknown"}</strong>
          <span>${payload.mode || ""} ${payload.band || ""} ${payload.frequency_hz || ""}</span>
          <small>${payload.started_at || ""}</small>
          <small>Notes: ${qso.note_history.length}</small>
          <div class="monitor-actions">
            ${
              qso.deleted
                ? `<button class="toolbar-button" type="button" data-qso-action="restore" data-qso-id="${qso.qso_id}">Restore</button>`
                : `<button class="toolbar-button" type="button" data-qso-action="delete" data-qso-id="${qso.qso_id}">Delete</button>
                   <button class="toolbar-button" type="button" data-qso-action="note" data-qso-id="${qso.qso_id}">Add Note</button>`
            }
          </div>
        </article>`;
      })
      .join("")}
  </div>`;
}

async function refreshQsos(includeDeleted = false) {
  const payload = await fetch(`/api/qsos?include_deleted=${includeDeleted}`).then((response) => response.json());
  state.qsos = payload.qsos;
}

async function submitQsoCreate(event) {
  event.preventDefault();
  const form = new FormData(event.currentTarget);
  const frequency = form.get("frequency_hz")?.toString().trim();
  const payload = {
    contacted_callsign: form.get("contacted_callsign")?.toString() || "",
    mode: form.get("mode")?.toString() || "",
    band: form.get("band")?.toString() || "",
    notes: form.get("notes")?.toString() || "",
    frequency_hz: frequency ? Number(frequency) : null,
  };
  const response = await fetch("/api/qso/create", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  const result = await response.json();
  if (!response.ok) {
    state.qsoError = result.error || "QSO proposal rejected";
  } else {
    state.qsoError = "";
    event.currentTarget.reset();
  }
  await refreshQsos();
  await refreshRuntimeEvents();
  render();
}

async function runQsoAction(action, qsoId) {
  let endpoint = `/api/qso/${action}`;
  let payload = { qso_id: qsoId };
  if (action === "note") {
    const note = window.prompt("Add note to QSO");
    if (!note) return;
    endpoint = "/api/qso/note";
    payload = { qso_id: qsoId, note };
  }
  await fetch(endpoint, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  await refreshQsos(action === "restore");
  await refreshRuntimeEvents();
  render();
}

async function importAdifFromPrompt() {
  const path = window.prompt("Path to ADIF file to import");
  if (!path) return;
  const response = await fetch("/api/adif/import", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ path }),
  });
  state.importSummary = await response.json();
  await refreshQsos();
  await refreshRuntimeEvents();
  openScreen("import-summary");
  render();
}

async function exportAdifFromPrompt() {
  const path = window.prompt("Path to write ADIF export");
  if (!path) return;
  const response = await fetch("/api/adif/export", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ path, include_deleted: false }),
  });
  state.importSummary = await response.json();
  await refreshRuntimeEvents();
  openScreen("import-summary");
}

async function verifyLogChain() {
  state.importSummary = await fetch("/api/log/verify").then((response) => response.json());
  openScreen("import-summary");
}

async function rebuildProjections() {
  state.importSummary = await fetch("/api/projections/rebuild", { method: "POST" }).then((response) => response.json());
  await refreshQsos();
  openScreen("import-summary");
}

function renderSyncStatus() {
  const sync = state.syncState;
  if (!sync) return `<p class="muted">Sync state loading.</p>`;
  return `<div class="sync-panel">
    <p><strong>LAN discovery:</strong> ${sync.discovery_running ? "running" : "stopped"}</p>
    <p><strong>Local identity:</strong> ${sync.identity.display_name}<br /><small>${sync.identity.device_id}</small></p>
    <div class="monitor-actions">
      <button id="sync-start-discovery" class="toolbar-button" type="button">Start</button>
      <button id="sync-stop-discovery" class="toolbar-button" type="button">Stop</button>
      <button id="sync-refresh-peers" class="toolbar-button" type="button">Refresh Peers</button>
      <button id="sync-handshake" class="toolbar-button" type="button">Handshake</button>
      <button id="sync-copy-identity" class="toolbar-button" type="button">Copy Identity</button>
    </div>
    <div class="qso-list">
      ${
        sync.peers.length
          ? sync.peers
              .map(
                (peer) => `<button class="event-row ${state.selectedPeerId === peer.peer_id ? "is-selected" : ""}" type="button" data-peer-id="${peer.peer_id}">
                  <span class="event-main"><strong>${peer.display_name}</strong><span>${peer.connection_state} / ${peer.sync_state}</span></span>
                  <span class="event-meta"><small>${peer.addresses.join(", ")}</small><small>${peer.protocol_version}</small></span>
                </button>`,
              )
              .join("")
          : `<p class="muted">No peers discovered yet.</p>`
      }
    </div>
    <pre class="path-block">${sync.latest_handshake ? JSON.stringify(sync.latest_handshake, null, 2) : "No handshake yet."}</pre>
  </div>`;
}

async function refreshSyncState() {
  state.syncState = await fetch("/api/sync/state").then((response) => response.json());
}

async function syncPost(path, body = {}) {
  const response = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const result = await response.json();
  await refreshSyncState();
  await refreshRuntimeEvents();
  render();
  return result;
}

function startDiscovery() {
  syncPost("/api/sync/discovery/start");
}

function stopDiscovery() {
  syncPost("/api/sync/discovery/stop");
}

function refreshPeers() {
  syncPost("/api/sync/peers/refresh");
}

function handshakeSelectedPeer() {
  syncPost("/api/sync/handshake", { peer_id: state.selectedPeerId });
}

function copyLocalSyncIdentity() {
  if (state.syncState) navigator.clipboard?.writeText(JSON.stringify(state.syncState.identity, null, 2));
}

boot().catch((error) => {
  state.busConnected = false;
  document.body.innerHTML = `<main class="screen"><div class="screen-body"><h1>GUI failed to start</h1><pre>${error}</pre></div></main>`;
});
