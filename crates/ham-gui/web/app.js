const state = {
  shell: null,
  commands: [],
  plugins: [],
  serviceProviders: { providers: [], preferred_providers: {} },
  credentials: null,
  netControl: null,
  mapState: null,
  permissionState: null,
  runtimeEvents: [],
  runtimeStatus: null,
  qsos: [],
  station: null,
  awards: null,
  search: { query: "", results: [] },
  uploads: null,
  activations: [],
  activeActivation: null,
  lookupSuggestion: null,
  acceptedLookupFields: null,
  rigStatus: null,
  acceptedRigFields: null,
  qsoError: "",
  duplicateWarning: "",
  importSummary: null,
  reportPreview: null,
  lastReport: null,
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
  state.serviceProviders = payload.service_providers || { providers: [], preferred_providers: {} };
  state.runtimeEvents = payload.runtime_events;
  state.runtimeStatus = payload.runtime_status;
  state.activeWorkspace = payload.shell.active_workspace;
  state.busConnected = payload.runtime_status.connected;

  bindShellControls();
  renderWorkspaceSelector();
  await refreshQsos();
  await refreshStation();
  await refreshAwards();
  await refreshUploads();
  await refreshCredentials();
  await refreshNetControl();
  await refreshMapState();
  await refreshActivations();
  await refreshRigStatus();
  await refreshSyncState();
  await refreshPluginPermissions();
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
    if ((event.ctrlKey || event.metaKey) && key === "l") {
      event.preventDefault();
      runCommand("focus.callsign-entry");
    }
    if (event.key === "Enter" && document.activeElement?.id === "callsign-entry-input") {
      const form = byId("qso-create-form");
      if (form?.checkValidity()) {
        event.preventDefault();
        form.requestSubmit();
      }
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
  const rigState = state.rigStatus?.active_state;
  const rigLabel = rigState ? `${rigState.frequency_hz || "freq?"} Hz ${rigState.mode || ""}` : "none";
  byId("status-sync").textContent = `Sync: ${state.runtimeStatus?.sync_state || "Local only"} / Rig: ${rigLabel}`;
  byId("status-events").textContent = `Runtime events: ${state.runtimeStatus?.runtime_event_count || state.runtimeEvents.length}`;
  byId("status-errors").textContent = `Errors: ${state.runtimeStatus?.latest_error_count || 0}`;
  byId("status-sync-peers").textContent = `Discovery: ${state.syncState?.discovery_running ? "running" : "stopped"} / ${state.syncState?.peers?.length || 0} peers / ${state.syncState?.warning_count || 0} warnings`;
  const mapStatus = state.mapState?.status || {};
  byId("status-map-grid").textContent = `Grid: ${mapStatus.grid || "unknown"}`;
  byId("status-map-coordinates").textContent = `Coords: ${formatCoordinate(mapStatus.coordinates)}`;
  byId("status-map-distance").textContent = `Distance: ${mapStatus.distance || "n/a"}`;
  byId("status-map-bearing").textContent = `Bearing: ${mapStatus.bearing || "n/a"}`;
  byId("status-map-zoom").textContent = `Zoom: ${mapStatus.zoom || "n/a"}`;
  byId("status-map-layer").textContent = `Layer: ${mapStatus.selected_layer || "none"}`;

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
    case "station-summary":
      return renderStationSummary();
    case "station-profiles":
      return renderStationProfiles();
    case "equipment-manager":
      return renderEquipmentManager();
    case "awards-summary":
      return renderAwardsSummary();
    case "global-search":
      return renderGlobalSearch();
    case "uploads":
      return renderUploads();
    case "interactive-map":
      return renderInteractiveMap();
    case "map-layers":
      return renderMapLayers();
    case "map-selected-object":
      return renderMapSelectedObject();
    case "map-search":
      return renderMapSearch();
    case "map-filters":
      return renderMapFilters();
    case "propagation":
      return renderPropagation();
    case "weather":
      return renderWeather();
    case "event-bus-monitor":
      return renderEventBusMonitor();
    case "activation-setup":
      return renderActivationSetup();
    case "activation-progress":
      return renderActivationProgress();
    case "activation-recent-qsos":
      return renderActivationRecentQsos();
    case "portable-logger-entry":
      return renderPortableLoggerEntry();
    case "spots-alerts":
      return `<p>Spots and alert feeds will connect to POTA/SOTA APIs in a future plugin update.</p>`;
    case "plugin-permissions":
      return state.plugins
        .map((plugin) => `<p><strong>${plugin.name}</strong><br />${permissionSummary(plugin.plugin_id)}</p>`)
        .join("");
    case "service-providers":
      return renderServiceProviders();
    case "credential-manager":
      return renderCredentialManager();
    case "net-session-control":
      return renderNetSessionControl();
    case "net-checkin-entry":
      return renderNetCheckinEntry();
    case "net-checkin-roster":
      return renderNetRoster();
    case "net-traffic-queue":
      return renderNetTrafficQueue();
    case "net-report":
      return renderNetReport();
    case "sync-status":
      return renderSyncStatus();
    case "rig-control":
      return renderRigControl();
    case "map-placeholder":
      return renderInteractiveMap();
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
  if (command.id === "services.open") openScreen("services");
  if (command.id === "services.cache.clear") clearServiceCache();
  if (command.id === "services.lookup.test") lookupCallsignFromPrompt();
  if (command.id === "services.spotting.test") openScreen("services");
  if (command.id === "credentials.open" || command.id === "credentials.create") openScreen("credentials");
  if (command.id === "credentials.test") testFirstCredential();
  if (command.id === "open.diagnostics") openScreen("diagnostics");
  if (command.id === "diagnostics.open-folder") openScreen("diagnostics-folder");
  if (command.id === "diagnostics.report.problem") openReportProblemScreen();
  if (command.id === "diagnostics.report.export") exportDiagnosticZipFromScreen();
  if (command.id === "diagnostics.report.upload") uploadDiagnosticReportFromScreen();
  if (command.id === "diagnostics.report.copy-last-id") copyLastReportId();
  if (command.id === "adif.import") importAdifFromPrompt();
  if (command.id === "adif.export") exportAdifFromPrompt();
  if (command.id === "lookup.callsign") lookupCallsignFromPrompt();
  if (command.id === "lookup.cache.clear") clearLookupCache();
  if (command.id === "lookup.provider-status") showLookupProviderStatus();
  if (command.id === "rig.connect") connectRig();
  if (command.id === "rig.disconnect") disconnectRig();
  if (command.id === "rig.refresh-state") refreshRigState();
  if (command.id === "rig.use-frequency-mode") acceptRigSuggestion(true);
  if (command.id === "rig.open-panel") switchWorkspace("casual-logger");
  if (command.id === "station.profiles.open") openScreen("station-profiles");
  if (command.id === "station.equipment.open") openScreen("equipment");
  if (command.id === "station.profile.switch") switchStationProfileFromPrompt();
  if (command.id === "awards.open") switchWorkspace("awards");
  if (command.id === "awards.rebuild") refreshAwards().then(render);
  if (command.id === "search.open" || command.id === "logger.open-advanced-search") openScreen("search");
  if (command.id === "search.deleted") runSearchPrompt("deleted:true ");
  if (command.id === "uploads.open") openScreen("uploads");
  if (command.id === "uploads.queue-not-uploaded") queueNotUploadedQsos();
  if (command.id === "uploads.export-adif") exportAdifFromPrompt();
  if (command.id === "map.open") switchWorkspace("maps");
  if (command.id === "map.layers.toggle") switchWorkspace("maps");
  if (command.id === "map.center.home") refreshMapState().then(() => switchWorkspace("maps"));
  if (command.id === "map.center.activation") refreshMapState().then(() => switchWorkspace("maps"));
  if (command.id === "map.center.selected-qso") refreshMapState().then(() => switchWorkspace("maps"));
  if (command.id === "map.grayline.toggle") toggleMapLayer("grayline");
  if (command.id === "map.distance.recalculate") refreshMapState().then(render);
  if (command.id === "propagation.open") switchWorkspace("maps");
  if (command.id === "logger.submit-qso") byId("qso-create-form")?.requestSubmit();
  if (command.id === "logger.clear-form") clearQsoForm();
  if (command.id === "logger.use-rig-frequency") acceptRigSuggestion(true);
  if (command.id === "logger.accept-lookup-suggestions") acceptLookupSuggestion();
  if (command.id === "logger.open-recent-qsos") switchWorkspace("casual-logger");
  if (command.id === "net.open") switchWorkspace("net-control");
  if (command.id === "net.session.start") startNetFromPrompt();
  if (command.id === "net.session.end") endActiveNet();
  if (command.id === "net.checkin.focus") {
    switchWorkspace("net-control");
    requestAnimationFrame(() => byId("net-checkin-callsign")?.focus());
  }
  if (command.id === "net.checkin.late") addLateCheckinFromPrompt();
  if (command.id === "net.report.export") exportNetReport();
  if (command.id === "net.traffic.open") switchWorkspace("net-control");
  if (command.id === "activation.start-pota") startActivationFromPrompt("pota");
  if (command.id === "activation.start-sota") startActivationFromPrompt("sota");
  if (command.id === "activation.end-current") endCurrentActivation();
  if (command.id === "activation.export-adif") exportActivationAdifFromPrompt();
  if (command.id === "activation.workspace") switchWorkspace("pota-sota");
  if (command.id === "official-log.verify-chain") verifyLogChain();
  if (command.id === "projection.rebuild") rebuildProjections();
  if (command.id === "sync.discovery.start") startDiscovery();
  if (command.id === "sync.discovery.stop") stopDiscovery();
  if (command.id === "sync.peers.refresh") refreshPeers();
  if (command.id === "sync.handshake.selected") handshakeSelectedPeer();
  if (command.id === "sync.preview-pull.selected") previewPullSelectedPeer();
  if (command.id === "sync.pull.selected") pullSelectedPeer();
  if (command.id === "sync.verify-local-chain") verifyLogChain();
  if (command.id === "sync.rebuild-projections") rebuildProjections();
  if (command.id === "sync.diagnostics.copy") copySyncDiagnosticSummary();
  if (command.id === "sync.cloud.connect") connectCloudSyncFromPrompt();
  if (command.id === "sync.cloud.push") pushCloudEvents();
  if (command.id === "sync.cloud.preview-pull") previewCloudPull();
  if (command.id === "sync.cloud.pull") pullCloudEvents();
  if (command.id === "sync.cloud.settings") openScreen("settings");
  if (command.id === "sync.cloud.diagnostics.copy") copyCloudSyncDiagnosticSummary();
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
    bindPluginPermissionControls();
    return;
  }

  if (kind === "services") {
    eyebrow.textContent = "Provider Runtime";
    title.textContent = "Service Providers";
    body.innerHTML = renderServiceProviderScreen();
    return;
  }

  if (kind === "credentials") {
    eyebrow.textContent = "Security";
    title.textContent = "Credential Manager";
    body.innerHTML = renderCredentialManager();
    bindCredentialControls();
    return;
  }

  if (kind === "station-profiles") {
    eyebrow.textContent = "Station";
    title.textContent = "Station Profiles";
    body.innerHTML = renderStationProfiles();
    return;
  }

  if (kind === "equipment") {
    eyebrow.textContent = "Station";
    title.textContent = "Equipment Manager";
    body.innerHTML = renderEquipmentManager();
    return;
  }

  if (kind === "search") {
    eyebrow.textContent = "Search";
    title.textContent = "Advanced Search";
    body.innerHTML = renderGlobalSearch();
    bindSearchControls();
    return;
  }

  if (kind === "uploads") {
    eyebrow.textContent = "Uploads";
    title.textContent = "Upload Queue";
    body.innerHTML = renderUploads();
    bindUploadControls();
    return;
  }

  if (kind === "diagnostics") {
    eyebrow.textContent = "Diagnostics";
    title.textContent = "Diagnostic Report";
    body.innerHTML = `<div class="monitor-actions">
        <button class="toolbar-button" type="button" onclick="openReportProblemScreen()">Report a Problem</button>
        <button class="toolbar-button" type="button" onclick="openScreen('diagnostics-folder')">Open Diagnostics Folder</button>
      </div>
      <div class="stack">${state.runtimeEvents.map(renderRuntimeEvent).join("")}</div>`;
    return;
  }

  if (kind === "report-problem") {
    eyebrow.textContent = "Diagnostics";
    title.textContent = "Report a Problem";
    body.innerHTML = renderReportProblem();
    bindReportProblemControls();
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
  byId("cloud-connect-settings")?.addEventListener("click", connectCloudSyncFromSettings);
}

function closeScreen() {
  byId("overlay").hidden = true;
}

function renderPluginManager() {
  const permissionState = state.permissionState;
  return `<p class="muted">Enable and disable controls are placeholders until real plugin loading is implemented. Permission grants are active and enforced.</p>
    <div class="plugin-grid">
      ${state.plugins
        .map(
          (plugin) => `<article class="plugin-card">
            <h3>${plugin.name}</h3>
            <p class="muted">${plugin.plugin_id}</p>
            <p>${permissionSummary(plugin.plugin_id)}</p>
            <details>
              <summary>Review permissions</summary>
              <div class="stack">${(plugin.requested_permissions || [])
                .map((permission) => renderPermissionReview(plugin.plugin_id, permission))
                .join("")}</div>
            </details>
            <button class="toolbar-button" disabled>${plugin.enabled ? "Enabled" : "Disabled"}</button>
            <button class="toolbar-button" type="button" data-approve-low-risk="${plugin.plugin_id}">Approve Low Risk</button>
          </article>`,
        )
        .join("")}
    </div>`;
}

function permissionSummary(pluginId) {
  const grants = permissionGrantsFor(pluginId);
  const counts = grants.reduce((acc, grant) => {
    acc[grant.status] = (acc[grant.status] || 0) + 1;
    return acc;
  }, {});
  return `<span class="pill">granted ${counts.granted || 0}</span><span class="pill">pending ${counts.pending || 0}</span><span class="pill">denied ${counts.denied || 0}</span><span class="pill">revoked ${counts.revoked || 0}</span>`;
}

function renderPermissionReview(pluginId, permissionId) {
  const metadata = permissionMetadata(permissionId);
  const grant = permissionGrant(pluginId, permissionId);
  const status = grant?.status || "pending";
  const risk = metadata?.risk_level || "unknown";
  return `<article class="sync-summary">
    <p><strong>${metadata?.display_name || permissionId}</strong> <span class="pill">${permissionId}</span> <span class="severity severity-${risk === "critical" || risk === "high" ? "error" : risk === "medium" ? "warn" : "info"}">${risk}</span></p>
    <p>${metadata?.user_visible_reason || "Plugin requested this permission."}</p>
    <p>Status: <strong>${status}</strong>${risk === "high" || risk === "critical" ? ` <span class="event-error">Admin review recommended</span>` : ""}</p>
    <div class="monitor-actions">
      <button class="toolbar-button" type="button" data-permission-action="grant" data-plugin-id="${pluginId}" data-permission-id="${permissionId}">Grant</button>
      <button class="toolbar-button" type="button" data-permission-action="deny" data-plugin-id="${pluginId}" data-permission-id="${permissionId}">Deny</button>
      <button class="toolbar-button" type="button" data-permission-action="revoke" data-plugin-id="${pluginId}" data-permission-id="${permissionId}">Revoke</button>
    </div>
  </article>`;
}

function permissionMetadata(permissionId) {
  return state.permissionState?.registry?.find((permission) => permission.permission_id === permissionId);
}

function permissionGrant(pluginId, permissionId) {
  return state.permissionState?.grants?.grants?.find((grant) => grant.plugin_id === pluginId && grant.permission_id === permissionId);
}

function permissionGrantsFor(pluginId) {
  return state.permissionState?.grants?.grants?.filter((grant) => grant.plugin_id === pluginId) || [];
}

function bindPluginPermissionControls() {
  document.querySelectorAll("[data-permission-action]").forEach((button) => {
    button.addEventListener("click", () => updatePluginPermission(button.dataset.permissionAction, button.dataset.pluginId, button.dataset.permissionId));
  });
  document.querySelectorAll("[data-approve-low-risk]").forEach((button) => {
    button.addEventListener("click", () => approveLowRiskPermissions(button.dataset.approveLowRisk));
  });
}

function bindCredentialControls() {
  byId("credential-create-form")?.addEventListener("submit", submitCredentialCreate);
  document.querySelectorAll("[data-credential-test]").forEach((button) => {
    button.addEventListener("click", () => testCredential(button.dataset.credentialTest));
  });
  document.querySelectorAll("[data-credential-revoke]").forEach((button) => {
    button.addEventListener("click", () => revokeCredential(button.dataset.credentialRevoke));
  });
}

function renderSettings() {
  const sections = ["General", "Appearance", "Callsign/Profile", "Sync", "Service Providers", "Credentials", "Lookup/Enrichment", "Rig Control", "Plugin Permissions", "Plugins", "Diagnostics", "Keyboard Shortcuts"];
  return `<div class="settings-grid">
    ${sections
      .map((section) =>
        section === "Sync"
          ? `<article class="settings-card"><h3>${section}</h3>${renderCloudSettings()}</article>`
          : section === "Service Providers"
            ? `<article class="settings-card"><h3>${section}</h3>${renderServiceProviderSummary()}</article>`
          : section === "Credentials"
            ? `<article class="settings-card"><h3>${section}</h3>${renderCredentialSummary()}</article>`
          : section === "Lookup/Enrichment"
            ? `<article class="settings-card"><h3>${section}</h3>${renderLookupSettings()}</article>`
          : section === "Rig Control"
            ? `<article class="settings-card"><h3>${section}</h3>${renderRigSettings()}</article>`
          : section === "Plugin Permissions"
            ? `<article class="settings-card"><h3>${section}</h3>${renderPermissionSettings()}</article>`
          : `<article class="settings-card"><h3>${section}</h3><p class="muted">Placeholder settings surface.</p></article>`,
      )
      .join("")}
  </div>`;
}

function renderServiceProviders() {
  const providers = [...(state.serviceProviders?.providers || [])].sort((left, right) =>
    `${left.metadata.service_type}:${left.metadata.display_name}`.localeCompare(`${right.metadata.service_type}:${right.metadata.display_name}`),
  );
  if (!providers.length) return `<p class="muted">No service providers are registered.</p>`;
  return `<div class="stack">${providers
    .map((provider) => {
      const meta = provider.metadata;
      const health = provider.health || {};
      const missingConfig = (meta.required_config_keys || []).length ? `<p class="event-warn">Missing config: ${meta.required_config_keys.join(", ")}</p>` : "";
      return `<article class="sync-summary">
        <p><strong>${meta.display_name}</strong> <span class="pill">${meta.service_type}</span></p>
        <p class="muted">${meta.provider_id} / ${meta.source_plugin_id}</p>
        <p>Status: ${provider.enabled ? "enabled" : "disabled"} / Health: ${health.state || "unknown"} / Priority: ${meta.priority}</p>
        <p>${meta.supports_offline ? "Offline capable" : "Online only"} / ${meta.requires_network_access ? "Network required" : "No network required"}</p>
        <p>Capabilities: ${(meta.capabilities || []).join(", ") || "none"}</p>
        <p>Permissions: ${(meta.required_permissions || []).join(", ") || "none"}</p>
        ${missingConfig}
      </article>`;
    })
    .join("")}</div>`;
}

function renderServiceProviderSummary() {
  const providers = state.serviceProviders?.providers || [];
  const byType = providers.reduce((acc, provider) => {
    const type = provider.metadata?.service_type || "unknown";
    acc[type] = (acc[type] || 0) + 1;
    return acc;
  }, {});
  return `<p>${providers.length} providers registered.</p>
    <p>${Object.entries(byType)
      .map(([type, count]) => `${type}: ${count}`)
      .join(" / ")}</p>
    <button class="toolbar-button" type="button" onclick="openScreen('services')">Review Providers</button>
    <button class="toolbar-button" type="button" onclick="clearServiceCache()">Clear Service Cache</button>`;
}

function renderServiceProviderScreen() {
  return `<p class="muted">Providers are registered through the shared core service framework. Provider configs reference credential IDs; raw secrets are handled only by the credential store.</p>
    ${renderServiceProviderSummary()}
    ${renderServiceProviders()}`;
}

function renderCredentialSummary() {
  const backend = state.credentials?.backend || {};
  const count = state.credentials?.credentials?.length || 0;
  const warning = backend.dev_only ? `<p class="event-error">Development fallback is active. Do not use for production credentials.</p>` : "";
  return `<p>${count} credential metadata records.</p>
    <p>Backend: ${backend.backend_name || "unknown"} / ${backend.available ? "available" : "unavailable"} / ${backend.secure ? "secure" : "not secure"}</p>
    ${warning}
    <button class="toolbar-button" type="button" onclick="openScreen('credentials')">Open Credential Manager</button>`;
}

function renderCredentialManager() {
  const backend = state.credentials?.backend || {};
  const credentials = state.credentials?.credentials || [];
  return `<div class="stack">
    <div class="sync-summary">
      <p><strong>${backend.backend_name || "Credential backend"}</strong></p>
      <p>${backend.message || "Credential backend status is unknown."}</p>
      <p>${backend.available ? "Available" : "Unavailable"} / ${backend.secure ? "Secure" : "Not secure"}${backend.dev_only ? " / Development only" : ""}</p>
      ${backend.dev_only ? `<p class="event-error">Secrets are stored in the explicit insecure development fallback. Set up OS keychain support before real provider use.</p>` : ""}
    </div>
    <form id="credential-create-form" class="qso-form">
      <label>Provider ID <input name="provider_id" class="placeholder-control" placeholder="qrz-stub" required /></label>
      <label>Account ID <input name="account_id" class="placeholder-control" placeholder="local-account" required /></label>
      <label>Service Type
        <select name="service_type" class="placeholder-control">
          <option value="callsign_lookup">callsign_lookup</option>
          <option value="log_upload">log_upload</option>
          <option value="authentication">authentication</option>
        </select>
      </label>
      <label>Label <input name="label" class="placeholder-control" placeholder="QRZ token" required /></label>
      <label>Secret <input name="secret" class="placeholder-control" type="password" autocomplete="new-password" placeholder="Secret is never displayed after save" required /></label>
      <button class="toolbar-button" type="submit" ${backend.available ? "" : "disabled"}>Save Credential</button>
    </form>
    <div class="qso-list">${credentials
      .map((credential) => `<article class="qso-row">
        <strong>${credential.label}</strong>
        <span>${credential.provider_id} / ${credential.service_type}</span>
        <small>${credential.status}</small>
        <div class="monitor-actions">
          <button class="toolbar-button" type="button" data-credential-test="${credential.credential_id}">Test</button>
          <button class="toolbar-button" type="button" data-credential-revoke="${credential.credential_id}">Revoke</button>
        </div>
      </article>`)
      .join("") || `<p class="muted">No credential metadata records.</p>`}</div>
  </div>`;
}

function renderPermissionSettings() {
  const settings = state.permissionState?.settings || {};
  return `<div class="qso-form">
    <label>Auto-grant built-in low/medium risk <input type="checkbox" ${settings.auto_grant_builtin_low_risk_permissions !== false ? "checked" : ""} disabled /></label>
    <label>Confirm high-risk permissions <input type="checkbox" ${settings.require_confirmation_for_high_risk_permissions !== false ? "checked" : ""} disabled /></label>
    <label>Allow external network plugins <input type="checkbox" ${settings.allow_external_network_plugins ? "checked" : ""} disabled /></label>
    <label>Permission audit logging <input type="checkbox" ${settings.permission_audit_logging !== false ? "checked" : ""} disabled /></label>
  </div>`;
}

function renderRigSettings() {
  const config = state.rigStatus?.config || {};
  return `<div class="qso-form">
    <label>Rig Control Enabled <input type="checkbox" ${config.enable_rig_control !== false ? "checked" : ""} disabled /></label>
    <label>Default Provider <input class="placeholder-control" value="${config.default_provider || "mock"}" disabled /></label>
    <label>Polling Interval ms <input class="placeholder-control" value="${config.polling_interval_ms || 1000}" disabled /></label>
    <label>Auto Fill From Rig <input type="checkbox" ${config.auto_fill_from_rig !== false ? "checked" : ""} disabled /></label>
    <label>Hamlib Host/Port <input class="placeholder-control" value="${config.hamlib_endpoint || "127.0.0.1:4532"}" disabled /></label>
    <label>Serial Settings <input class="placeholder-control" value="${config.serial_settings_placeholder || "planned"}" disabled /></label>
  </div>`;
}

function renderLookupSettings() {
  return `<div class="qso-form">
    <label>Lookup Enabled <input type="checkbox" checked disabled /></label>
    <label>Online Lookup <input type="checkbox" disabled /></label>
    <label>Preferred Provider <input class="placeholder-control" value="local-prefix" disabled /></label>
    <label>Cache TTL Days <input class="placeholder-control" value="30" disabled /></label>
    <label>Provider Credentials <input class="placeholder-control" value="Not stored in MVP" disabled /></label>
    <button class="toolbar-button" type="button" onclick="clearLookupCache()">Clear Lookup Cache</button>
  </div>`;
}

function renderCloudSettings() {
  const config = state.syncState?.cloud_config || {};
  return `<div class="qso-form">
    <label>Cloud Sync <input id="cloud-enabled" type="checkbox" ${config.enable_cloud_sync ? "checked" : ""} /></label>
    <label>Server URL <input id="cloud-server-url" class="placeholder-control" value="${config.sync_server_url || "http://127.0.0.1:9740"}" /></label>
    <label>Device Name <input id="cloud-device-name" class="placeholder-control" value="${config.device_name || "KE8YGW Logger Device"}" /></label>
    <label>Prefer LAN <input id="cloud-prefer-lan" type="checkbox" ${config.prefer_lan_sync !== false ? "checked" : ""} /></label>
    <label>Auto Push <input id="cloud-auto-push" type="checkbox" ${config.auto_push_enabled ? "checked" : ""} /></label>
    <label>Auto Pull <input id="cloud-auto-pull" type="checkbox" ${config.auto_pull_enabled ? "checked" : ""} /></label>
    <label>Sync Interval Seconds <input id="cloud-sync-interval" class="placeholder-control" inputmode="numeric" value="${config.sync_interval_seconds || 300}" /></label>
    <button id="cloud-connect-settings" class="toolbar-button" type="button">Pair / Connect</button>
  </div>`;
}

function renderReportProblem() {
  const preview = state.reportPreview;
  const lastReportId = state.lastReport?.report_id || state.lastReport?.upload?.report_id || "";
  return `<div class="qso-form">
    <label>Report Type
      <select id="report-type" class="placeholder-control">
        <option value="basic" ${preview?.report_type === "basic" ? "selected" : ""}>Basic</option>
        <option value="sync" ${preview?.report_type === "sync" ? "selected" : ""}>Sync</option>
      </select>
    </label>
    <label>Short Description
      <input id="report-description" class="placeholder-control" placeholder="What went wrong?" />
    </label>
    <label>User Notes
      <textarea id="report-notes" class="placeholder-control" rows="5" placeholder="Steps, expectations, anything useful"></textarea>
    </label>
    <label>Export Path
      <input id="report-output-path" class="placeholder-control" placeholder="C:\\Temp\\ham-report.zip" />
    </label>
    <div class="monitor-actions">
      <button id="report-refresh-preview" class="toolbar-button" type="button">Preview</button>
      <button id="report-export" class="toolbar-button" type="button">Export ZIP</button>
      <button id="report-upload" class="toolbar-button" type="button">Upload Report</button>
      <button id="report-copy-id" class="toolbar-button" type="button" ${lastReportId ? "" : "disabled"}>Copy Report ID</button>
    </div>
    <div class="sync-summary">
      <p><strong>Included files</strong></p>
      <p>${preview?.included_files?.map((file) => `<span class="pill">${file}</span>`).join("") || "Click Preview to inspect bundle contents."}</p>
      <p><strong>Redaction summary</strong></p>
      <p>${preview ? redactionSummaryText(preview.redaction_summary) : "Official logs, credentials, full AI payloads, private profile fields, and raw provider metadata are excluded/redacted by default."}</p>
      ${lastReportId ? `<p><strong>Last report ID:</strong> <span class="pill">${lastReportId}</span></p>` : ""}
      ${state.importSummary ? `<pre class="path-block">${JSON.stringify(state.importSummary, null, 2)}</pre>` : ""}
    </div>
  </div>`;
}

function bindReportProblemControls() {
  byId("report-refresh-preview")?.addEventListener("click", refreshReportPreview);
  byId("report-export")?.addEventListener("click", exportDiagnosticZipFromScreen);
  byId("report-upload")?.addEventListener("click", uploadDiagnosticReportFromScreen);
  byId("report-copy-id")?.addEventListener("click", copyLastReportId);
  byId("report-type")?.addEventListener("change", refreshReportPreview);
}

function redactionSummaryText(summary) {
  if (!summary) return "";
  return `${summary.secret_fields_redacted || 0} secret-like fields redacted; ${summary.private_profile_fields_redacted || 0} private profile fields redacted. Excluded: ${(summary.categories_removed || []).join(", ")}.`;
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
    byId("callsign-entry-input")?.addEventListener("input", updateDuplicateWarning);
    byId("lookup-callsign-button")?.addEventListener("click", () => lookupCallsign(byId("callsign-entry-input")?.value || ""));
    byId("accept-lookup-button")?.addEventListener("click", acceptLookupSuggestion);
    byId("use-rig-button")?.addEventListener("click", () => acceptRigSuggestion(true));
    byId("refresh-rig-button")?.addEventListener("click", refreshRigState);
  }
  const activationForm = byId("activation-start-form");
  if (activationForm) activationForm.addEventListener("submit", submitActivationStart);
  const portableForm = byId("portable-qso-form");
  if (portableForm) portableForm.addEventListener("submit", submitPortableQsoCreate);
  byId("portable-lookup-button")?.addEventListener("click", () => lookupCallsign(byId("portable-callsign-input")?.value || ""));
  byId("portable-accept-lookup-button")?.addEventListener("click", acceptLookupSuggestion);
  byId("portable-use-rig-button")?.addEventListener("click", () => acceptRigSuggestion(true));
  byId("portable-refresh-rig-button")?.addEventListener("click", refreshRigState);
  byId("activation-end-button")?.addEventListener("click", endCurrentActivation);
  byId("rig-connect-button")?.addEventListener("click", connectRig);
  byId("rig-disconnect-button")?.addEventListener("click", disconnectRig);
  byId("rig-refresh-button")?.addEventListener("click", refreshRigState);
  byId("rig-use-button")?.addEventListener("click", () => acceptRigSuggestion(true));
  byId("rig-mock-apply-button")?.addEventListener("click", applyMockRigSettings);
  byId("credential-create-form")?.addEventListener("submit", submitCredentialCreate);
  document.querySelectorAll("[data-credential-test]").forEach((button) => {
    button.addEventListener("click", () => testCredential(button.dataset.credentialTest));
  });
  document.querySelectorAll("[data-credential-revoke]").forEach((button) => {
    button.addEventListener("click", () => revokeCredential(button.dataset.credentialRevoke));
  });
  byId("net-session-start-form")?.addEventListener("submit", submitNetSessionStart);
  byId("net-end-button")?.addEventListener("click", endActiveNet);
  byId("net-checkin-form")?.addEventListener("submit", submitNetCheckin);
  byId("net-traffic-form")?.addEventListener("submit", submitNetTraffic);
  byId("net-report-export")?.addEventListener("click", exportNetReport);
  document.querySelectorAll("[data-net-checkin-delete]").forEach((button) => {
    button.addEventListener("click", () => deleteNetCheckin(button.dataset.netCheckinDelete));
  });

  document.querySelectorAll("[data-qso-action]").forEach((button) => {
    button.addEventListener("click", () => {
      runQsoAction(button.dataset.qsoAction, button.dataset.qsoId);
    });
  });
  document.querySelectorAll("[data-map-layer]").forEach((control) => {
    control.addEventListener("change", () => toggleMapLayer(control.dataset.mapLayer, control.checked));
  });
  byId("map-refresh-button")?.addEventListener("click", () => refreshMapState().then(render));

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
    byId("sync-preview-pull").addEventListener("click", previewPullSelectedPeer);
    byId("sync-pull-events").addEventListener("click", pullSelectedPeer);
    byId("sync-copy-identity").addEventListener("click", copyLocalSyncIdentity);
    byId("cloud-connect").addEventListener("click", connectCloudSyncFromPrompt);
    byId("cloud-push").addEventListener("click", pushCloudEvents);
    byId("cloud-preview").addEventListener("click", previewCloudPull);
    byId("cloud-pull").addEventListener("click", pullCloudEvents);
  }
  bindSearchControls();
  bindUploadControls();
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

function renderRigControl() {
  const status = state.rigStatus;
  if (!status) return `<p class="muted">Rig state loading.</p>`;
  const device = status.devices?.[0] || {};
  const rig = status.active_state || {};
  return `<div class="qso-form">
    <label>Provider
      <select class="placeholder-control" aria-label="Rig provider" disabled>
        <option selected>MockRigProvider</option>
        <option>HamlibProviderStub</option>
      </select>
    </label>
    <p><strong>${device.display_name || "Mock HF Rig"}</strong><br /><small>${device.provider || "mock"} / ${device.connection_type || "mock"}</small></p>
    <p>Status: ${device.connection_status || "disconnected"}</p>
    <p>Frequency: ${rig.frequency_hz || "unknown"} Hz</p>
    <p>Band / Mode: ${rig.band || "unknown"} / ${rig.mode || "unknown"} ${rig.submode || ""}</p>
    <p>Split: ${rig.split_enabled ? "on" : "off"} / PTT: ${rig.ptt ? "on" : "off"}</p>
    <p>Last update: ${rig.timestamp ? new Date(rig.timestamp).toLocaleTimeString() : "never"}</p>
    ${device.error ? `<p class="event-error">${device.error}</p>` : ""}
    <div class="monitor-actions">
      <button id="rig-connect-button" class="toolbar-button" type="button">Connect</button>
      <button id="rig-disconnect-button" class="toolbar-button" type="button">Disconnect</button>
      <button id="rig-refresh-button" class="toolbar-button" type="button">Refresh</button>
      <button id="rig-use-button" class="toolbar-button" type="button" ${status.autofill_suggestion ? "" : "disabled"}>Use In Logger</button>
    </div>
    <label>Mock Frequency Hz
      <input id="rig-mock-frequency" class="placeholder-control" inputmode="numeric" value="${rig.frequency_hz || 14250000}" />
    </label>
    <label>Mock Mode
      <input id="rig-mock-mode" class="placeholder-control" value="${rig.mode || "SSB"}" />
    </label>
    <label>Mock PTT <input id="rig-mock-ptt" type="checkbox" ${rig.ptt ? "checked" : ""} /></label>
    <button id="rig-mock-apply-button" class="toolbar-button" type="button">Apply Mock State</button>
    <p class="muted">Hamlib is represented by a build-safe stub for now; no radio hardware is required.</p>
  </div>`;
}

function renderCallsignEntry() {
  return `<form id="qso-create-form" class="qso-form">
      ${renderStationSummary()}
      <label>Contacted callsign
        <input id="callsign-entry-input" name="contacted_callsign" class="placeholder-control" aria-label="Contacted callsign" placeholder="K1ABC" required />
      </label>
      <p id="duplicate-warning" class="event-error" ${state.duplicateWarning ? "" : "hidden"}>${state.duplicateWarning || ""}</p>
      <div class="monitor-actions">
        <button id="lookup-callsign-button" class="toolbar-button" type="button">Lookup</button>
        <button id="accept-lookup-button" class="toolbar-button" type="button" ${state.lookupSuggestion ? "" : "disabled"}>Accept Suggestions</button>
      </div>
      ${renderLookupSuggestion()}
      <label>Mode
        <input name="mode" class="placeholder-control" aria-label="Mode" placeholder="SSB" value="${state.acceptedRigFields?.mode || ""}" required />
      </label>
      <label>Frequency Hz
        <input name="frequency_hz" class="placeholder-control" aria-label="Frequency Hz" placeholder="14250000" inputmode="numeric" value="${state.acceptedRigFields?.frequency_hz || ""}" />
      </label>
      <label>Band
        <input name="band" class="placeholder-control" aria-label="Band" placeholder="20m" value="${state.acceptedRigFields?.band || ""}" />
      </label>
      <div class="monitor-actions">
        <button id="refresh-rig-button" class="toolbar-button" type="button">Refresh Rig</button>
        <button id="use-rig-button" class="toolbar-button" type="button" ${state.rigStatus?.autofill_suggestion ? "" : "disabled"}>Use Rig Frequency/Mode</button>
      </div>
      ${renderRigSuggestion()}
      <label>Notes
        <input name="notes" class="placeholder-control" aria-label="Notes" placeholder="Optional note" />
      </label>
      <button class="toolbar-button" type="submit">Submit QSO Proposal</button>
      ${state.qsoError ? `<p class="event-error">${state.qsoError}</p>` : ""}
    </form>
    <p>Submits a proposal to ham-core; the GUI does not write official events directly.</p>`;
}

function renderActivationSetup() {
  const active = state.activeActivation;
  return `<form id="activation-start-form" class="qso-form">
      <label>Activation Type
        <select name="activation_type" class="placeholder-control">
          <option value="pota">POTA</option>
          <option value="sota">SOTA</option>
          <option value="portable">Generic Portable</option>
        </select>
      </label>
      <label>Park/Summit Reference
        <input name="reference" class="placeholder-control" placeholder="US-1234 or W8O/NE-001" required />
      </label>
      <label>Station Callsign
        <input name="station_callsign" class="placeholder-control" value="KE8YGW" required />
      </label>
      <label>Operator Callsign
        <input name="operator_callsign" class="placeholder-control" value="KE8YGW" required />
      </label>
      <label>Grid / Location
        <input name="grid" class="placeholder-control" placeholder="EN91" />
      </label>
      <label>Notes
        <input name="notes" class="placeholder-control" placeholder="Optional activation note" />
      </label>
      <button class="toolbar-button" type="submit">Start Activation</button>
    </form>
    <div class="monitor-actions">
      <button id="activation-end-button" class="toolbar-button" type="button" ${active ? "" : "disabled"}>End Current Activation</button>
    </div>`;
}

function renderActivationProgress() {
  const active = state.activeActivation;
  if (!active) return `<p class="muted">No active activation.</p>`;
  const started = active.payload.started_at ? new Date(active.payload.started_at) : null;
  const elapsed = started ? `${Math.max(0, Math.round((Date.now() - started.getTime()) / 60000))} min` : "unknown";
  const reference = active.payload.park_id || active.payload.summit_id || active.payload.reference || "";
  return `<div class="sync-summary">
    <p><strong>${active.payload.activation_type?.toUpperCase() || "Portable"} ${reference}</strong></p>
    <p>Status: ${active.status}</p>
    <p>Elapsed: ${elapsed}</p>
    <p>QSOs: ${active.qso_count} / Unique: ${active.unique_callsign_count}</p>
    <p>Bands: ${Object.entries(active.band_summary).map(([band, count]) => `${band} ${count}`).join(", ") || "none"}</p>
    <p>Modes: ${Object.entries(active.mode_summary).map(([mode, count]) => `${mode} ${count}`).join(", ") || "none"}</p>
  </div>`;
}

function renderActivationRecentQsos() {
  const active = state.activeActivation;
  if (!active) return `<p class="muted">Start an activation to see linked QSOs.</p>`;
  const qsos = state.qsos.filter((qso) => qso.payload.activation_id === active.activation_id);
  if (!qsos.length) return `<p class="muted">No QSOs linked to this activation yet.</p>`;
  return `<div class="qso-list">${qsos
    .map((qso) => `<article class="qso-row"><strong>${qso.payload.contacted_callsign}</strong><span>${qso.payload.mode || ""} ${qso.payload.band || ""}</span><small>${qso.payload.started_at || ""}</small></article>`)
    .join("")}</div>`;
}

function renderPortableLoggerEntry() {
  const active = state.activeActivation;
  return `<form id="portable-qso-form" class="qso-form">
      <p class="muted">${active ? `Logging against ${active.payload.park_id || active.payload.summit_id || active.activation_id}` : "No active activation; QSO will be portable-source only."}</p>
      <label>Callsign <input id="portable-callsign-input" name="contacted_callsign" class="placeholder-control" required /></label>
      <div class="monitor-actions">
        <button id="portable-lookup-button" class="toolbar-button" type="button">Lookup</button>
        <button id="portable-accept-lookup-button" class="toolbar-button" type="button" ${state.lookupSuggestion ? "" : "disabled"}>Accept Suggestions</button>
      </div>
      ${renderLookupSuggestion()}
      <label>Mode <input name="mode" class="placeholder-control" value="${state.acceptedRigFields?.mode || "SSB"}" required /></label>
      <label>Band <input name="band" class="placeholder-control" placeholder="20m" value="${state.acceptedRigFields?.band || ""}" /></label>
      <label>Frequency Hz <input name="frequency_hz" class="placeholder-control" inputmode="numeric" value="${state.acceptedRigFields?.frequency_hz || ""}" /></label>
      <div class="monitor-actions">
        <button id="portable-refresh-rig-button" class="toolbar-button" type="button">Refresh Rig</button>
        <button id="portable-use-rig-button" class="toolbar-button" type="button" ${state.rigStatus?.autofill_suggestion ? "" : "disabled"}>Use Rig Frequency/Mode</button>
      </div>
      ${renderRigSuggestion()}
      <label>Notes <input name="notes" class="placeholder-control" /></label>
      <button class="toolbar-button" type="submit">Submit Portable QSO</button>
    </form>`;
}

function renderLookupSuggestion() {
  const suggestion = state.lookupSuggestion;
  if (!suggestion) return `<p class="muted">Lookup suggestions will appear here and are not written until accepted.</p>`;
  const fields = suggestion.suggested_fields || {};
  return `<div class="sync-summary">
    <p><strong>${suggestion.normalized_callsign}</strong> from ${suggestion.provider} (${Math.round((suggestion.confidence || 0) * 100)}%)</p>
    <p>${fields.name || ""} ${fields.qth || ""}</p>
    <p>${fields.grid || ""} ${fields.country || ""} ${fields.dxcc ? `DXCC ${fields.dxcc}` : ""}</p>
  </div>`;
}

function renderRigSuggestion() {
  const suggestion = state.rigStatus?.autofill_suggestion;
  if (!suggestion) return `<p class="muted">Connect or refresh a rig to suggest frequency, band, and mode.</p>`;
  return `<div class="sync-summary">
    <p><strong>Rig suggestion</strong> from ${suggestion.source}</p>
    <p>${suggestion.frequency_hz || "unknown"} Hz / ${suggestion.band || "unknown"} / ${suggestion.mode || "unknown"} ${suggestion.submode || ""}</p>
    <p class="muted">${state.acceptedRigFields ? "Accepted into this form. Submit to create an official QSO proposal." : "Advisory only until accepted."}</p>
  </div>`;
}

function renderStationSummary() {
  const profile = activeStationProfile();
  const config = activeStationConfiguration();
  if (!profile) return `<p class="muted">No station profile configured.</p>`;
  return `<div class="sync-summary">
    <p><strong>${profile.display_name}</strong></p>
    <p>${profile.station_callsign} ${profile.operator_callsign ? `/ ${profile.operator_callsign}` : ""}</p>
    <p>${profile.default_grid || ""} ${profile.default_qth || ""}</p>
    <p>${config ? `Config: ${config.name}` : "No configuration selected"}</p>
    <div class="monitor-actions">
      <button class="toolbar-button" type="button" onclick="openScreen('station-profiles')">Profiles</button>
      <button class="toolbar-button" type="button" onclick="openScreen('equipment')">Equipment</button>
    </div>
  </div>`;
}

function renderStationProfiles() {
  const profiles = state.station?.profiles || [];
  if (!profiles.length) return `<p class="muted">No station profiles yet.</p>`;
  return `<div class="qso-list">${profiles
    .map((profile) => `<article class="qso-row">
      <strong>${profile.display_name}</strong>
      <span>${profile.station_callsign} ${profile.operator_callsign ? `/ ${profile.operator_callsign}` : ""}</span>
      <small>${profile.default_grid || ""} ${profile.active ? "Active" : ""}</small>
      <button class="toolbar-button" type="button" onclick="selectStationProfile('${profile.station_profile_id}')">Use Profile</button>
    </article>`)
    .join("")}</div>`;
}

function renderEquipmentManager() {
  const equipment = state.station?.equipment || [];
  if (!equipment.length) return `<p class="muted">No equipment records yet.</p>`;
  return `<div class="qso-list">${equipment
    .map((item) => `<article class="qso-row">
      <strong>${item.display_name}</strong>
      <span>${item.equipment_type} ${item.manufacturer || ""} ${item.model || ""}</span>
      <small>${item.status}</small>
    </article>`)
    .join("")}</div>`;
}

function renderAwardsSummary() {
  const progress = state.awards?.progress || [];
  if (!progress.length) return `<p class="muted">Award progress will appear after QSOs are logged.</p>`;
  return `<div class="plugin-grid">${progress
    .map((award) => `<article class="plugin-card">
      <h3>${award.name}</h3>
      <p><strong>${award.credit_count}</strong> credits</p>
      <p class="muted">${award.confirmed_credit_count} confirmed</p>
      <details><summary>Credits</summary>
        <div class="stack">${(award.credits || []).slice(0, 20).map((credit) => `<span class="pill">${credit.display_name}</span>`).join("")}</div>
      </details>
    </article>`)
    .join("")}</div>`;
}

function renderGlobalSearch() {
  const rows = state.search?.results || [];
  return `<div class="stack">
    <form id="global-search-form" class="qso-form">
      <label>Search
        <input id="global-search-input" class="placeholder-control" name="query" value="${state.search?.query || ""}" placeholder="callsign:K1ABC band:20m portable" />
      </label>
      <button class="toolbar-button" type="submit">Search</button>
    </form>
    <div class="qso-list">${rows
      .map((result) => `<article class="qso-row">
        <strong>${result.payload.contacted_callsign || result.qso_id}</strong>
        <span>${result.payload.mode || ""} ${result.payload.band || ""} ${result.deleted ? "(deleted)" : ""}</span>
        <small>${(result.matched_fields || []).join(", ")}</small>
      </article>`)
      .join("") || `<p class="muted">No search results.</p>`}</div>
  </div>`;
}

function renderUploads() {
  const queue = state.uploads || { targets: [], jobs: [] };
  return `<div class="stack">
    <div class="monitor-actions">
      <button class="toolbar-button" type="button" data-upload-all>Queue All Not Uploaded</button>
      <button class="toolbar-button" type="button" onclick="exportAdifFromPrompt()">Export Upload ADIF</button>
    </div>
    <h4>Targets</h4>
    <div class="qso-list">${(queue.targets || [])
      .map((target) => `<article class="qso-row"><strong>${target.display_name}</strong><span>${target.provider_id}</span><small>${target.enabled ? "Enabled" : "Disabled"}</small></article>`)
      .join("")}</div>
    <h4>Jobs</h4>
    <div class="qso-list">${(queue.jobs || [])
      .map((job) => `<article class="qso-row"><strong>${job.target_id}</strong><span>${job.status}</span><small>${job.qso_ids.length} QSOs</small></article>`)
      .join("") || `<p class="muted">No upload jobs queued.</p>`}</div>
  </div>`;
}

function renderNetSessionControl() {
  const active = state.netControl?.active_session;
  return `<div class="stack">
    <form id="net-session-start-form" class="qso-form">
      <label>Net Name <input name="net_name" class="placeholder-control" value="ARES Weekly Net" required /></label>
      <label>Station Callsign <input name="station_callsign" class="placeholder-control" value="${activeStationProfile()?.station_callsign || "KE8YGW"}" required /></label>
      <label>Net Control Operator <input name="net_control_operator_id" class="placeholder-control" value="${activeStationProfile()?.operator_callsign || activeStationProfile()?.station_callsign || "KE8YGW"}" required /></label>
      <label>Frequency Hz <input name="frequency_hz" class="placeholder-control" inputmode="numeric" value="${state.acceptedRigFields?.frequency_hz || ""}" /></label>
      <label>Band <input name="band" class="placeholder-control" value="${state.acceptedRigFields?.band || ""}" /></label>
      <label>Mode <input name="mode" class="placeholder-control" value="${state.acceptedRigFields?.mode || "FM"}" /></label>
      <label>Notes <input name="notes" class="placeholder-control" /></label>
      <button class="toolbar-button" type="submit" ${active ? "disabled" : ""}>Start Net</button>
    </form>
    <div class="sync-summary">
      <p><strong>${active?.payload?.net_name || "No active net"}</strong></p>
      <p>Status: ${active?.status || "inactive"} / Check-ins: ${active?.checkin_count || 0} / Traffic: ${active?.traffic_count || 0}</p>
      <button id="net-end-button" class="toolbar-button" type="button" ${active ? "" : "disabled"}>End Net</button>
    </div>
  </div>`;
}

function renderNetCheckinEntry() {
  const active = state.netControl?.active_session;
  return `<form id="net-checkin-form" class="qso-form">
    <p class="muted">${active ? `Checking into ${active.payload.net_name}` : "Start a net before accepting check-ins."}</p>
    <label>Callsign <input id="net-checkin-callsign" name="callsign" class="placeholder-control" placeholder="K1ABC" /></label>
    <label>Name <input name="operator_name" class="placeholder-control" /></label>
    <label>Location <input name="location" class="placeholder-control" /></label>
    <label>Grid <input name="grid" class="placeholder-control" /></label>
    <label>Tactical Callsign <input name="tactical_callsign" class="placeholder-control" /></label>
    <label>Status
      <select name="status" class="placeholder-control">
        <option value="checked_in">Checked In</option>
        <option value="late">Late</option>
        <option value="excused">Excused</option>
        <option value="left">Left</option>
      </select>
    </label>
    <label>Traffic
      <select name="traffic" class="placeholder-control">
        <option value="none">None</option>
        <option value="listed">Listed</option>
        <option value="priority">Priority</option>
        <option value="emergency">Emergency</option>
      </select>
    </label>
    <label>Notes <input name="notes" class="placeholder-control" /></label>
    <button class="toolbar-button" type="submit" ${active ? "" : "disabled"}>Submit Check-In</button>
  </form>`;
}

function renderNetRoster() {
  const checkins = state.netControl?.checkins || [];
  const warnings = state.netControl?.active_session?.duplicate_warnings || [];
  return `<div class="stack">
    ${warnings.map((warning) => `<p class="event-error">${warning}</p>`).join("")}
    <div class="qso-list">${checkins
      .map((checkin) => `<article class="qso-row">
        <strong>${checkin.payload.callsign || checkin.payload.tactical_callsign || "Tactical only"}</strong>
        <span>${checkin.payload.operator_name || ""} ${checkin.payload.location || ""}</span>
        <small>${checkin.status} / ${checkin.traffic}</small>
        <button class="toolbar-button" type="button" data-net-checkin-delete="${checkin.checkin_id}">Delete</button>
      </article>`)
      .join("") || `<p class="muted">No check-ins yet.</p>`}</div>
  </div>`;
}

function renderNetTrafficQueue() {
  const active = state.netControl?.active_session;
  const traffic = state.netControl?.traffic || [];
  return `<div class="stack">
    <form id="net-traffic-form" class="qso-form">
      <label>From <input name="from_callsign" class="placeholder-control" /></label>
      <label>To <input name="to_callsign" class="placeholder-control" /></label>
      <label>Precedence
        <select name="precedence" class="placeholder-control">
          <option value="routine">Routine</option>
          <option value="priority">Priority</option>
          <option value="emergency">Emergency</option>
        </select>
      </label>
      <label>Summary <input name="summary" class="placeholder-control" required /></label>
      <button class="toolbar-button" type="submit" ${active ? "" : "disabled"}>Add Traffic</button>
    </form>
    <div class="qso-list">${traffic
      .map((item) => `<article class="qso-row ${item.precedence === "emergency" ? "event-error" : ""}">
        <strong>${item.precedence}</strong>
        <span>${item.summary}</span>
        <small>${item.status || "listed"}</small>
      </article>`)
      .join("") || `<p class="muted">No listed traffic.</p>`}</div>
  </div>`;
}

function renderNetReport() {
  const active = state.netControl?.active_session;
  return `<div class="stack">
    <div class="monitor-actions">
      <button id="net-report-export" class="toolbar-button" type="button" ${active ? "" : "disabled"}>Export Report Event</button>
    </div>
    <pre class="path-block">${state.netControl?.report_preview || "Start a net to build a report."}</pre>
  </div>`;
}

function renderRecentQsos() {
  if (!state.qsos.length) {
    return `<p class="muted">No visible QSOs yet. Create one from Callsign Entry.</p>`;
  }
  return `<div class="qso-list">
    ${state.qsos
      .map((qso) => {
        const payload = qso.payload;
        const activation = state.activations.find((activation) => activation.activation_id === payload.activation_id);
        const activationLabel = activation ? activation.payload.park_id || activation.payload.summit_id || activation.payload.reference || "Activation" : "";
        return `<article class="qso-row">
          <strong>${payload.contacted_callsign || "Unknown"}</strong>
          <span>${payload.mode || ""} ${payload.band || ""} ${payload.frequency_hz || ""} ${activationLabel ? `<span class="pill">${activationLabel}</span>` : ""}</span>
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

function renderInteractiveMap() {
  const map = state.mapState;
  if (!map) return `<p class="muted">Map state is loading.</p>`;
  const qsoMarkers = (map.qso_objects || []).filter((object) => object.marker);
  const paths = (map.qso_objects || []).filter((object) => object.path);
  const activeLayers = (map.layers?.layers || []).filter((layer) => layer.enabled);
  return `<div class="map-canvas" role="img" aria-label="Operator map preview">
      <div class="map-grid-lines"></div>
      <div class="map-hud">
        <strong>${map.status?.grid || "Grid unknown"}</strong>
        <span>${formatCoordinate(map.status?.coordinates)}</span>
        <span>${qsoMarkers.length} QSO markers / ${paths.length} paths</span>
      </div>
      <div class="map-marker-cloud">
        ${qsoMarkers
          .slice(0, 12)
          .map((object, index) => `<span class="map-dot" style="left:${12 + ((index * 17) % 76)}%;top:${18 + ((index * 29) % 64)}%" title="${object.marker.title}"></span>`)
          .join("")}
        ${(map.station_markers || [])
          .slice(0, 6)
          .map((marker, index) => `<span class="map-dot station-dot" style="left:${45 + index * 5}%;top:${42 + index * 4}%" title="${marker.title}"></span>`)
          .join("")}
      </div>
    </div>
    <div class="metric-grid">
      <span>Distance ${map.status?.distance || "n/a"}</span>
      <span>Bearing ${map.status?.bearing || "n/a"}</span>
      <span>Zoom ${map.status?.zoom || "n/a"}</span>
      <span>Layer ${map.status?.selected_layer || "none"}</span>
    </div>
    <p class="muted">Enabled layers: ${activeLayers.map((layer) => layer.title).join(", ") || "none"}</p>
    <button id="map-refresh-button" class="toolbar-button" type="button">Refresh Map State</button>`;
}

function renderMapLayers() {
  const layers = state.mapState?.layers?.layers || [];
  if (!layers.length) return `<p class="muted">No map layers are registered.</p>`;
  return `<div class="stack">${layers
    .sort((left, right) => left.order - right.order)
    .map((layer) => `<label class="check-row">
      <input type="checkbox" data-map-layer="${layer.layer_id}" ${layer.enabled ? "checked" : ""} />
      <span><strong>${layer.title}</strong><br /><small>${layer.kind} / ${layer.source_plugin_id}</small></span>
    </label>`)
    .join("")}</div>`;
}

function renderMapSelectedObject() {
  const selected = (state.mapState?.qso_objects || []).find((object) => object.marker) || null;
  if (!selected) return `<p class="muted">Select a mapped QSO or station to inspect it.</p>`;
  const marker = selected.marker;
  return `<div class="sync-summary">
    <p><strong>${marker.title}</strong></p>
    <p>${marker.description}</p>
    <p>Grid: ${selected.grid || "unknown"} / Entity: ${selected.entity || "unknown"}</p>
    <p>Distance: ${selected.distance ? `${selected.distance.kilometers.toFixed(1)} km` : "n/a"}</p>
    <p>Bearing: ${selected.bearing ? `${selected.bearing.initial_degrees.toFixed(0)} deg` : "n/a"}</p>
    <p>Layer: ${marker.layer_id}</p>
  </div>`;
}

function renderMapSearch() {
  const providers = (state.mapState?.providers || []).filter((provider) => provider.metadata?.service_type === "geocoding");
  return `<label>Map Search
      <input class="placeholder-control" type="search" placeholder="Grid, callsign, park, summit, or place" aria-label="Map search" />
    </label>
    <p class="muted">Geocoding providers: ${providers.map((provider) => provider.metadata.display_name).join(", ") || "none"}</p>`;
}

function renderMapFilters() {
  return `<div class="filter-grid">
    <label>Band<input class="placeholder-control" placeholder="20m" /></label>
    <label>Mode<input class="placeholder-control" placeholder="FT8" /></label>
    <label>Date<input class="placeholder-control" placeholder="2026-07-01..2026-07-06" /></label>
    <label>Entity<input class="placeholder-control" placeholder="Japan" /></label>
  </div>
  <p class="muted">Filters are modeled for QSO, station, activation, and future APRS/satellite overlays.</p>`;
}

function renderPropagation() {
  const forecast = state.mapState?.propagation;
  if (!forecast) return `<p class="muted">Propagation data is unavailable.</p>`;
  return `<div class="metric-grid">
      <span>SFI ${forecast.solar?.sfi ?? "n/a"}</span>
      <span>A ${forecast.solar?.a_index ?? "n/a"}</span>
      <span>K ${forecast.solar?.k_index ?? "n/a"}</span>
      <span>X-ray ${forecast.solar?.xray_class || "n/a"}</span>
    </div>
    <div class="qso-list">${(forecast.bands || [])
      .map((band) => `<article class="qso-row"><strong>${band.band}</strong><span>${band.day_rating} day / ${band.night_rating} night</span><small>${band.notes || ""}</small></article>`)
      .join("")}</div>`;
}

function renderWeather() {
  const weather = state.mapState?.weather;
  if (!weather) return `<p class="muted">Weather data is unavailable.</p>`;
  return `<div class="metric-grid">
      <span>${weather.temperature_c ?? "n/a"} C</span>
      <span>${weather.wind?.speed_kph ?? "n/a"} kph ${weather.wind?.direction_degrees ?? ""} deg</span>
      <span>${weather.conditions || "unknown"}</span>
      <span>${weather.provider_id}</span>
    </div>
    <p class="muted">Lightning and radar overlays are placeholders for future providers.</p>`;
}

function formatCoordinate(coordinate) {
  if (!coordinate) return "unknown";
  const lat = Number(coordinate.latitude).toFixed(3);
  const lon = Number(coordinate.longitude).toFixed(3);
  return `${lat}, ${lon}`;
}

async function refreshMapState() {
  state.mapState = await fetch("/api/maps/state").then((response) => response.json());
}

async function toggleMapLayer(layerId, enabled = null) {
  const layer = (state.mapState?.layers?.layers || []).find((candidate) => candidate.layer_id === layerId);
  const nextEnabled = enabled ?? !layer?.enabled;
  state.mapState = await fetch("/api/maps/layer/toggle", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ layer_id: layerId, enabled: nextEnabled }),
  }).then((response) => response.json());
  await refreshRuntimeEvents();
  render();
}

async function refreshQsos(includeDeleted = false) {
  const payload = await fetch(`/api/qsos?include_deleted=${includeDeleted}`).then((response) => response.json());
  state.qsos = payload.qsos;
}

async function refreshActivations() {
  const payload = await fetch("/api/activations").then((response) => response.json());
  state.activations = payload.activations;
  state.activeActivation = payload.active_activation;
}

async function refreshPluginPermissions() {
  state.permissionState = await fetch("/api/plugins/permissions").then((response) => response.json());
}

async function refreshStation() {
  state.station = await fetch("/api/station").then((response) => response.json());
}

async function refreshAwards() {
  state.awards = await fetch("/api/awards").then((response) => response.json());
}

async function refreshUploads() {
  state.uploads = await fetch("/api/uploads").then((response) => response.json());
}

async function refreshCredentials() {
  state.credentials = await fetch("/api/credentials").then((response) => response.json());
}

async function refreshNetControl() {
  state.netControl = await fetch("/api/net-control").then((response) => response.json());
}

function activeStationProfile() {
  const id = state.station?.active_profile_id;
  return (state.station?.profiles || []).find((profile) => profile.station_profile_id === id) || null;
}

function activeStationConfiguration() {
  const id = state.station?.active_configuration_id;
  return (state.station?.configurations || []).find((config) => config.configuration_id === id) || null;
}

async function selectStationProfile(stationProfileId) {
  await fetch("/api/station/select-profile", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ station_profile_id: stationProfileId }),
  });
  await refreshStation();
  await refreshRuntimeEvents();
  render();
}

function switchStationProfileFromPrompt() {
  const profiles = state.station?.profiles || [];
  const callsign = window.prompt(`Station profile callsign (${profiles.map((profile) => profile.station_callsign).join(", ")})`);
  const profile = profiles.find((candidate) => candidate.station_callsign.toLowerCase() === (callsign || "").toLowerCase());
  if (profile) selectStationProfile(profile.station_profile_id);
}

function bindSearchControls() {
  byId("global-search-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const query = new FormData(event.currentTarget).get("query")?.toString() || "";
    await runSearch(query);
    openScreen("search");
  });
}

async function runSearch(query) {
  state.search.query = query;
  state.search = await fetch(`/api/search?q=${encodeURIComponent(query)}`).then((response) => response.json());
  state.search.query = query;
  await refreshRuntimeEvents();
}

function runSearchPrompt(prefix = "") {
  const query = window.prompt("Search QSOs", prefix);
  if (query == null) return;
  runSearch(query).then(() => openScreen("search"));
}

function bindUploadControls() {
  document.querySelectorAll("[data-upload-all]").forEach((button) => {
    button.addEventListener("click", queueNotUploadedQsos);
  });
}

async function queueNotUploadedQsos() {
  const target = state.uploads?.targets?.[0]?.target_id;
  if (!target) return;
  state.importSummary = await fetch("/api/uploads/queue", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ target_id: target, qso_ids: [], all_not_uploaded: true }),
  }).then((response) => response.json());
  await refreshUploads();
  await refreshRuntimeEvents();
  openScreen("uploads");
}

async function submitCredentialCreate(event) {
  event.preventDefault();
  const form = new FormData(event.currentTarget);
  const payload = {
    provider_id: form.get("provider_id")?.toString() || "",
    account_id: form.get("account_id")?.toString() || "local-account",
    service_type: form.get("service_type")?.toString() || "log_upload",
    label: form.get("label")?.toString() || "Credential",
    secret: form.get("secret")?.toString() || "",
    metadata: { source: "gui" },
  };
  state.importSummary = await fetch("/api/credentials/create", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  }).then((response) => response.json());
  await refreshCredentials();
  await refreshRuntimeEvents();
  openScreen("credentials");
}

async function testCredential(credentialId) {
  state.importSummary = await fetch("/api/credentials/test", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ credential_id: credentialId }),
  }).then((response) => response.json());
  await refreshCredentials();
  await refreshRuntimeEvents();
  openScreen("credentials");
}

async function testFirstCredential() {
  const credentialId = state.credentials?.credentials?.[0]?.credential_id;
  if (credentialId) await testCredential(credentialId);
  else openScreen("credentials");
}

async function revokeCredential(credentialId) {
  state.importSummary = await fetch("/api/credentials/revoke", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ credential_id: credentialId }),
  }).then((response) => response.json());
  await refreshCredentials();
  await refreshRuntimeEvents();
  openScreen("credentials");
}

async function submitNetSessionStart(event) {
  event.preventDefault();
  const form = new FormData(event.currentTarget);
  const payload = {
    net_name: form.get("net_name")?.toString() || "Net",
    station_callsign: form.get("station_callsign")?.toString() || "KE8YGW",
    net_control_operator_id: form.get("net_control_operator_id")?.toString() || "KE8YGW",
    frequency_hz: numberOrNull(form.get("frequency_hz")),
    band: emptyToNull(form.get("band")),
    mode: emptyToNull(form.get("mode")),
    notes: emptyToNull(form.get("notes")),
  };
  state.importSummary = await fetch("/api/net/session/start", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  }).then((response) => response.json());
  await refreshNetControl();
  await refreshRuntimeEvents();
  render();
}

function startNetFromPrompt() {
  switchWorkspace("net-control");
  requestAnimationFrame(() => byId("net-session-start-form")?.requestSubmit());
}

async function endActiveNet() {
  const sessionId = state.netControl?.active_session?.net_session_id;
  if (!sessionId) return;
  state.importSummary = await fetch("/api/net/session/end", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ credential_id: sessionId }),
  }).then((response) => response.json());
  await refreshNetControl();
  await refreshRuntimeEvents();
  render();
}

async function submitNetCheckin(event) {
  event.preventDefault();
  const active = state.netControl?.active_session;
  if (!active) return;
  const form = new FormData(event.currentTarget);
  const payload = {
    net_session_id: active.net_session_id,
    callsign: emptyToNull(form.get("callsign")),
    operator_name: emptyToNull(form.get("operator_name")),
    location: emptyToNull(form.get("location")),
    grid: emptyToNull(form.get("grid")),
    tactical_callsign: emptyToNull(form.get("tactical_callsign")),
    status: form.get("status")?.toString() || "checked_in",
    traffic: form.get("traffic")?.toString() || "none",
    notes: emptyToNull(form.get("notes")),
  };
  state.importSummary = await fetch("/api/net/checkin/create", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  }).then((response) => response.json());
  await refreshNetControl();
  await refreshRuntimeEvents();
  render();
}

async function addLateCheckinFromPrompt() {
  const active = state.netControl?.active_session;
  if (!active) {
    switchWorkspace("net-control");
    return;
  }
  const callsign = window.prompt("Late check-in callsign");
  if (!callsign) return;
  state.importSummary = await fetch("/api/net/checkin/create", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      net_session_id: active.net_session_id,
      callsign,
      status: "late",
      traffic: "none",
    }),
  }).then((response) => response.json());
  await refreshNetControl();
  await refreshRuntimeEvents();
  render();
}

async function deleteNetCheckin(checkinId) {
  state.importSummary = await fetch("/api/net/checkin/delete", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ credential_id: checkinId }),
  }).then((response) => response.json());
  await refreshNetControl();
  await refreshRuntimeEvents();
  render();
}

async function submitNetTraffic(event) {
  event.preventDefault();
  const active = state.netControl?.active_session;
  if (!active) return;
  const form = new FormData(event.currentTarget);
  state.importSummary = await fetch("/api/net/traffic/create", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      net_session_id: active.net_session_id,
      from_callsign: emptyToNull(form.get("from_callsign")),
      to_callsign: emptyToNull(form.get("to_callsign")),
      precedence: form.get("precedence")?.toString() || "routine",
      summary: form.get("summary")?.toString() || "",
    }),
  }).then((response) => response.json());
  await refreshNetControl();
  await refreshRuntimeEvents();
  render();
}

async function exportNetReport() {
  const sessionId = state.netControl?.active_session?.net_session_id;
  if (!sessionId) return;
  state.importSummary = await fetch("/api/net/report/export", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ credential_id: sessionId }),
  }).then((response) => response.json());
  await refreshNetControl();
  await refreshRuntimeEvents();
  render();
}

function emptyToNull(value) {
  const text = value?.toString() || "";
  return text.trim() ? text : null;
}

function numberOrNull(value) {
  const text = value?.toString() || "";
  if (!text.trim()) return null;
  const parsed = Number.parseInt(text, 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function clearQsoForm() {
  byId("qso-create-form")?.reset();
  state.acceptedLookupFields = null;
  state.acceptedRigFields = null;
  state.qsoError = "";
  render();
}

async function updatePluginPermission(action, pluginId, permissionId) {
  const endpoint = `/api/plugins/permissions/${action}`;
  const response = await fetch(endpoint, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      plugin_id: pluginId,
      permission_id: permissionId,
      reason: "Local admin MVP decision",
    }),
  });
  const result = await response.json();
  if (response.ok && result.permissions) state.permissionState = result.permissions;
  state.importSummary = result;
  await refreshRuntimeEvents();
  openScreen("plugins");
}

async function approveLowRiskPermissions(pluginId) {
  const plugin = state.plugins.find((candidate) => candidate.plugin_id === pluginId);
  if (!plugin) return;
  for (const permissionId of plugin.requested_permissions || []) {
    const metadata = permissionMetadata(permissionId);
    if (metadata?.risk_level === "low") {
      await fetch("/api/plugins/permissions/grant", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          plugin_id: pluginId,
          permission_id: permissionId,
          reason: "Approved all low-risk permissions",
        }),
      });
    }
  }
  await refreshPluginPermissions();
  await refreshRuntimeEvents();
  openScreen("plugins");
}

async function refreshRigStatus() {
  state.rigStatus = await fetch("/api/rig/status").then((response) => response.json());
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
    ...(state.acceptedLookupFields || {}),
    ...(state.acceptedRigFields || {}),
  };
  updateDuplicateWarning();
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
  await refreshActivations();
  await refreshAwards();
  await refreshRigStatus();
  await refreshRuntimeEvents();
  state.acceptedLookupFields = null;
  state.acceptedRigFields = null;
  state.lookupSuggestion = null;
  state.duplicateWarning = "";
  render();
}

function updateDuplicateWarning() {
  const callsign = byId("callsign-entry-input")?.value?.trim()?.toUpperCase();
  if (!callsign) {
    state.duplicateWarning = "";
    return;
  }
  const duplicate = state.qsos.find((qso) => {
    const payload = qso.payload || {};
    return !qso.deleted && (payload.contacted_callsign || "").toUpperCase() === callsign;
  });
  state.duplicateWarning = duplicate
    ? `Possible duplicate: ${callsign} was logged ${duplicate.payload.started_at || "recently"}.`
    : "";
  const warning = byId("duplicate-warning");
  if (warning) {
    warning.textContent = state.duplicateWarning;
    warning.hidden = !state.duplicateWarning;
  }
}

async function submitPortableQsoCreate(event) {
  event.preventDefault();
  const form = new FormData(event.currentTarget);
  const frequency = form.get("frequency_hz")?.toString().trim();
  await fetch("/api/qso/portable-create", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      contacted_callsign: form.get("contacted_callsign")?.toString() || "",
      mode: form.get("mode")?.toString() || "",
      band: form.get("band")?.toString() || "",
      notes: form.get("notes")?.toString() || "",
      frequency_hz: frequency ? Number(frequency) : null,
      ...(state.acceptedLookupFields || {}),
      ...(state.acceptedRigFields || {}),
    }),
  });
  event.currentTarget.reset();
  await refreshQsos();
  await refreshActivations();
  await refreshAwards();
  await refreshRigStatus();
  await refreshRuntimeEvents();
  state.acceptedLookupFields = null;
  state.acceptedRigFields = null;
  state.lookupSuggestion = null;
  render();
}

async function lookupCallsign(callsign) {
  if (!callsign) return;
  const payload = await fetch(`/api/lookup/callsign?callsign=${encodeURIComponent(callsign)}`).then((response) => response.json());
  state.lookupSuggestion = payload.suggestion || null;
  await refreshRuntimeEvents();
  render();
}

function acceptLookupSuggestion() {
  if (!state.lookupSuggestion) return;
  const fields = state.lookupSuggestion.suggested_fields || {};
  state.acceptedLookupFields = {
    name: fields.name || null,
    qth: fields.qth || null,
    grid: fields.grid || null,
    country: fields.country || null,
    dxcc: fields.dxcc || null,
    cq_zone: fields.cq_zone || null,
    itu_zone: fields.itu_zone || null,
    lookup_source: fields.lookup_source || state.lookupSuggestion.provider,
    lookup_confidence: fields.lookup_confidence || state.lookupSuggestion.confidence,
    enriched_fields: fields.enriched_fields || [],
  };
  render();
}

function lookupCallsignFromPrompt() {
  const callsign = window.prompt("Callsign to look up");
  if (callsign) lookupCallsign(callsign);
}

async function clearLookupCache() {
  state.importSummary = await fetch("/api/lookup/cache/clear", { method: "POST" }).then((response) => response.json());
  await refreshServiceProviders();
  await refreshRuntimeEvents();
  openScreen("import-summary");
}

async function clearServiceCache() {
  state.importSummary = await fetch("/api/services/cache/clear", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({}),
  }).then((response) => response.json());
  await refreshServiceProviders();
  await refreshRuntimeEvents();
  openScreen("import-summary");
}

async function showLookupProviderStatus() {
  state.importSummary = await fetch("/api/lookup/status").then((response) => response.json());
  openScreen("import-summary");
}

async function refreshServiceProviders() {
  state.serviceProviders = await fetch("/api/services/providers").then((response) => response.json());
}

async function openReportProblemScreen() {
  await refreshReportPreview();
  openScreen("report-problem");
}

async function refreshReportPreview() {
  const reportType = byId("report-type")?.value || "basic";
  state.reportPreview = await fetch(`/api/diagnostics/report-preview?type=${encodeURIComponent(reportType)}`).then((response) =>
    response.json(),
  );
  if (!byId("report-type")) return;
  openScreen("report-problem");
}

async function exportDiagnosticZipFromScreen() {
  const reportType = byId("report-type")?.value || "basic";
  const path = byId("report-output-path")?.value || window.prompt("Path to write diagnostic ZIP");
  if (!path) return;
  const response = await fetch("/api/diagnostics/report/export", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      report_type: reportType,
      path,
      user_notes: byId("report-notes")?.value || "",
      short_description: byId("report-description")?.value || "",
    }),
  });
  state.importSummary = await response.json();
  await refreshRuntimeEvents();
  openScreen("report-problem");
}

async function uploadDiagnosticReportFromScreen() {
  const reportType = byId("report-type")?.value || "basic";
  const response = await fetch("/api/diagnostics/report/upload", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      report_type: reportType,
      user_notes: byId("report-notes")?.value || "",
      short_description: byId("report-description")?.value || "",
    }),
  });
  const result = await response.json();
  state.importSummary = result;
  if (response.ok) state.lastReport = result.upload;
  await refreshRuntimeEvents();
  openScreen("report-problem");
}

function copyLastReportId() {
  const reportId = state.lastReport?.report_id || state.lastReport?.upload?.report_id;
  if (reportId) navigator.clipboard?.writeText(reportId);
}

async function connectRig() {
  await fetch("/api/rig/connect", { method: "POST" });
  await refreshRigStatus();
  await refreshRuntimeEvents();
  render();
}

async function disconnectRig() {
  await fetch("/api/rig/disconnect", { method: "POST" });
  state.acceptedRigFields = null;
  await refreshRigStatus();
  await refreshRuntimeEvents();
  render();
}

async function refreshRigState() {
  await fetch("/api/rig/refresh", { method: "POST" });
  await refreshRigStatus();
  await refreshRuntimeEvents();
  render();
}

async function applyMockRigSettings() {
  const frequency = byId("rig-mock-frequency")?.value?.trim();
  const mode = byId("rig-mock-mode")?.value?.trim();
  await fetch("/api/rig/mock/set", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      frequency_hz: frequency ? Number(frequency) : null,
      mode: mode || null,
      ptt: Boolean(byId("rig-mock-ptt")?.checked),
    }),
  });
  await refreshRigStatus();
  await refreshRuntimeEvents();
  render();
}

function acceptRigSuggestion(explicit = false) {
  const suggestion = state.rigStatus?.autofill_suggestion;
  if (!suggestion) return;
  state.acceptedRigFields = {
    frequency_hz: suggestion.frequency_hz || null,
    band: suggestion.band || null,
    mode: suggestion.mode || null,
    submode: suggestion.submode || null,
    rig_source: explicit ? "rig/manual-refresh" : suggestion.source,
    rig_id: suggestion.rig_id || null,
    rig_enriched_fields: suggestion.suggested_fields || [],
  };
  render();
}

async function submitActivationStart(event) {
  event.preventDefault();
  const form = new FormData(event.currentTarget);
  await fetch("/api/activation/start", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      activation_type: form.get("activation_type")?.toString() || "pota",
      reference: form.get("reference")?.toString() || "",
      station_callsign: form.get("station_callsign")?.toString() || "KE8YGW",
      operator_callsign: form.get("operator_callsign")?.toString() || "KE8YGW",
      grid: form.get("grid")?.toString() || "",
      notes: form.get("notes")?.toString() || "",
    }),
  });
  event.currentTarget.reset();
  await refreshActivations();
  await refreshRuntimeEvents();
  render();
}

function startActivationFromPrompt(kind) {
  const reference = window.prompt(`${kind.toUpperCase()} reference`);
  if (!reference) return;
  const profile = activeStationProfile();
  fetch("/api/activation/start", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      activation_type: kind,
      reference,
      station_callsign: profile?.station_callsign || "KE8YGW",
      operator_callsign: profile?.operator_callsign || profile?.station_callsign || "KE8YGW",
    }),
  }).then(async () => {
    await refreshActivations();
    render();
  });
}

async function endCurrentActivation() {
  if (!state.activeActivation) return;
  await fetch("/api/activation/end", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ activation_id: state.activeActivation.activation_id }),
  });
  await refreshActivations();
  await refreshRuntimeEvents();
  render();
}

async function exportActivationAdifFromPrompt() {
  const path = window.prompt("Path to write activation ADIF export");
  if (!path) return;
  state.importSummary = await fetch("/api/activation/export-adif", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ path, include_deleted: false }),
  }).then((response) => response.json());
  await refreshRuntimeEvents();
  openScreen("import-summary");
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
  await refreshActivations();
  await refreshAwards();
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
  await refreshAwards();
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
      <button id="sync-preview-pull" class="toolbar-button" type="button">Preview Pull</button>
      <button id="sync-pull-events" class="toolbar-button" type="button">Pull Missing</button>
      <button id="sync-copy-identity" class="toolbar-button" type="button">Copy Identity</button>
    </div>
    <div class="sync-summary">
      <p><strong>Cloud:</strong> ${sync.cloud_connection_state} / ${sync.cloud_config?.sync_server_url || "not configured"}</p>
      <p><strong>Account:</strong> ${sync.cloud_account_id || "not paired"}</p>
      <p><strong>Cloud head:</strong> <small>${sync.cloud_status?.accessible_logbooks?.[0]?.head_hash || "unknown"}</small></p>
      <p><strong>Last cloud push:</strong> ${sync.last_cloud_push_time || "never"}</p>
      <p><strong>Last cloud pull:</strong> ${sync.last_cloud_pull_time || "never"}</p>
      ${sync.cloud_divergence ? `<p class="event-error"><strong>Cloud divergence:</strong> ${sync.cloud_divergence}</p>` : ""}
      ${
        sync.latest_cloud_preview
          ? `<p><strong>Cloud preview:</strong> ${sync.latest_cloud_preview.status} / ${sync.latest_cloud_preview.missing_event_count} events pending</p>`
          : ""
      }
      ${
        sync.latest_cloud_push
          ? `<p><strong>Cloud push:</strong> ${sync.latest_cloud_push.status} / accepted ${sync.latest_cloud_push.accepted_count}, rejected ${sync.latest_cloud_push.rejected_count}</p>`
          : ""
      }
      <div class="monitor-actions">
        <button id="cloud-connect" class="toolbar-button" type="button">Connect Cloud</button>
        <button id="cloud-push" class="toolbar-button" type="button">Push Now</button>
        <button id="cloud-preview" class="toolbar-button" type="button">Preview Cloud Pull</button>
        <button id="cloud-pull" class="toolbar-button" type="button">Pull Cloud</button>
      </div>
    </div>
    <div class="sync-summary">
      <p><strong>Local head:</strong> <small>${sync.local_head?.head_hash || "genesis"}</small></p>
      <p><strong>Remote head:</strong> <small>${sync.remote_head?.head_hash || "unknown"}</small></p>
      <p><strong>Last sync:</strong> ${sync.last_sync_time || "never"}</p>
      ${sync.divergence ? `<p class="event-error"><strong>Divergence:</strong> ${sync.divergence}</p>` : ""}
      ${
        sync.latest_preview
          ? `<p><strong>Preview:</strong> ${sync.latest_preview.status} / ${sync.latest_preview.missing_event_count} events available</p>`
          : ""
      }
      ${
        sync.latest_pull
          ? `<p><strong>Pull:</strong> ${sync.latest_pull.status} / accepted ${sync.latest_pull.accepted_count}, rejected ${sync.latest_pull.rejected_count}</p>`
          : ""
      }
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
    <pre class="path-block">${sync.latest_preview ? JSON.stringify(sync.latest_preview, null, 2) : "No pull preview yet."}</pre>
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

async function previewPullSelectedPeer() {
  const result = await syncPost("/api/sync/preview-pull", { peer_id: state.selectedPeerId });
  state.importSummary = result.preview || result;
}

async function pullSelectedPeer() {
  const result = await syncPost("/api/sync/pull-events", { peer_id: state.selectedPeerId });
  state.importSummary = result.pull || result;
  await refreshQsos();
  render();
}

function copyLocalSyncIdentity() {
  if (state.syncState) navigator.clipboard?.writeText(JSON.stringify(state.syncState.identity, null, 2));
}

async function connectCloudSyncFromSettings() {
  const payload = {
    server_url: byId("cloud-server-url")?.value,
    device_name: byId("cloud-device-name")?.value,
    pairing_code: window.prompt("Pairing code", "local-dev-pairing-code") || "",
    account_id: "local-account",
    user_id: "local-user",
    enable_cloud_sync: Boolean(byId("cloud-enabled")?.checked),
    prefer_lan_sync: Boolean(byId("cloud-prefer-lan")?.checked),
    auto_push_enabled: Boolean(byId("cloud-auto-push")?.checked),
    auto_pull_enabled: Boolean(byId("cloud-auto-pull")?.checked),
    sync_interval_seconds: Number(byId("cloud-sync-interval")?.value || 300),
  };
  await syncPost("/api/sync/cloud/connect", payload);
}

function connectCloudSyncFromPrompt() {
  const pairing_code = window.prompt("Cloud pairing code", "local-dev-pairing-code");
  if (!pairing_code) return;
  syncPost("/api/sync/cloud/connect", {
    pairing_code,
    account_id: "local-account",
    user_id: "local-user",
    enable_cloud_sync: true,
  });
}

async function pushCloudEvents() {
  const result = await syncPost("/api/sync/cloud/push");
  state.importSummary = result.push || result;
}

async function previewCloudPull() {
  const result = await syncPost("/api/sync/cloud/preview-pull");
  state.importSummary = result.preview || result;
}

async function pullCloudEvents() {
  const result = await syncPost("/api/sync/cloud/pull");
  state.importSummary = result.local_pull || result.server_pull || result;
  await refreshQsos();
  render();
}

function copyCloudSyncDiagnosticSummary() {
  if (!state.syncState) return;
  const summary = {
    config: state.syncState.cloud_config,
    connection_state: state.syncState.cloud_connection_state,
    account_id: state.syncState.cloud_account_id,
    status: state.syncState.cloud_status,
    latest_preview: state.syncState.latest_cloud_preview,
    latest_pull: state.syncState.latest_cloud_pull,
    latest_push: state.syncState.latest_cloud_push,
    divergence: state.syncState.cloud_divergence,
  };
  navigator.clipboard?.writeText(JSON.stringify(summary, null, 2));
}

function copySyncDiagnosticSummary() {
  if (state.syncState) navigator.clipboard?.writeText(JSON.stringify(state.syncState, null, 2));
}

boot().catch((error) => {
  state.busConnected = false;
  document.body.innerHTML = `<main class="screen"><div class="screen-body"><h1>GUI failed to start</h1><pre>${error}</pre></div></main>`;
});
