import Network
import SwiftData
import SwiftUI

#if os(iOS)
import UIKit
#endif

enum FeatureDestination: String, CaseIterable, Identifiable, Hashable {
    case dashboard
    case logging
    case callsign
    case stations
    case providers
    case account
    case admin
    case maps
    case pota
    case sota
    case netControl
    case emergency
    case sync
    case backup
    case diagnostics
    case settings

    var id: String { rawValue }

    var title: String {
        switch self {
        case .dashboard: return "Dashboard"
        case .logging: return "Logging"
        case .callsign: return "Callsign"
        case .stations: return "Stations"
        case .providers: return "Providers"
        case .account: return "Account"
        case .admin: return "Administration"
        case .maps: return "Maps"
        case .pota: return "POTA"
        case .sota: return "SOTA"
        case .netControl: return "Net Control"
        case .emergency: return "Emergency"
        case .sync: return "Sync"
        case .backup: return "Backup"
        case .diagnostics: return "Diagnostics"
        case .settings: return "Settings"
        }
    }

    var systemImage: String {
        switch self {
        case .dashboard: return "gauge.with.dots.needle.67percent"
        case .logging: return "square.and.pencil"
        case .callsign: return "person.text.rectangle"
        case .stations: return "antenna.radiowaves.left.and.right"
        case .providers: return "point.3.connected.trianglepath.dotted"
        case .account: return "person.crop.circle.badge.checkmark"
        case .admin: return "person.badge.key"
        case .maps: return "map"
        case .pota: return "tree"
        case .sota: return "mountain.2"
        case .netControl: return "person.3.sequence"
        case .emergency: return "cross.case"
        case .sync: return "arrow.triangle.2.circlepath"
        case .backup: return "externaldrive"
        case .diagnostics: return "stethoscope"
        case .settings: return "gearshape"
        }
    }
}

struct AppShellView: View {
    @State private var selection: FeatureDestination? = .dashboard

    var body: some View {
        NavigationSplitView {
            List(FeatureDestination.allCases, selection: $selection) { destination in
                Label(destination.title, systemImage: destination.systemImage)
                    .tag(destination)
            }
            .navigationTitle("KE8YGW")
        } detail: {
            NavigationStack {
                destinationView(selection ?? .dashboard)
            }
        }
    }

    @ViewBuilder
    private func destinationView(_ destination: FeatureDestination) -> some View {
        switch destination {
        case .dashboard:
            DashboardView(selection: $selection)
        case .logging:
            LoggingWorkspaceView()
        case .callsign:
            CallsignLookupView()
        case .stations:
            StationManagementView()
        case .providers:
            ProviderStatusView()
        case .account:
            AccountWorkspaceView()
        case .admin:
            AdminWorkspaceView()
        case .maps:
            MapWorkspaceView()
        case .pota:
            POTAView()
        case .sota:
            SOTAView()
        case .netControl:
            NetControlView()
        case .emergency:
            EmergencyCommsView()
        case .sync:
            SyncWorkspaceView()
        case .backup:
            BackupRestoreView()
        case .diagnostics:
            DiagnosticsView()
        case .settings:
            SettingsView()
        }
    }
}

struct DashboardView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query(sort: \QSO.contactDate, order: .reverse) private var qsos: [QSO]
    @Query private var profiles: [StationProfile]
    @Query private var settings: [AppSettings]
    @StateObject private var deviceStatus = DeviceStatusViewModel()
    @Binding var selection: FeatureDestination?

    private var activeProfile: StationProfile? {
        profiles.first { $0.isActive } ?? profiles.first
    }
    private var appSettings: AppSettings? { settings.first }

    var body: some View {
        Group {
            switch activeLayout {
            case .liquidGlass: liquidGlassBody
            case .groupedLogbook: groupedLogbookBody
            case .mapSheet: mapSheetBody
            }
        }
        .navigationTitle(activeLayout.title)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Picker("Dashboard layout", selection: layoutBinding) {
                        ForEach(DashboardLayout.allCases) { layout in
                            Label(layout.title, systemImage: layout.systemImage).tag(layout)
                        }
                    }
                    Divider()
                    Picker("Appearance", selection: appearanceBinding) {
                        ForEach(AppearanceMode.allCases) { mode in
                            Text(mode.title).tag(mode)
                        }
                    }
                } label: {
                    Label("Dashboard layout", systemImage: "square.on.square.dashed")
                }
                .accessibilityLabel("Change dashboard layout or appearance")
            }
        }
        .task {
            await bridge.refreshAll()
            deviceStatus.start()
        }
    }

    private var activeLayout: DashboardLayout {
        appSettings?.effectiveDashboardLayout ?? .liquidGlass
    }

    /// Switching writes straight through to the shared settings record, so the
    /// choice survives a relaunch and reaches the Rust settings store with the
    /// operator's next save.
    private var layoutBinding: Binding<DashboardLayout> {
        Binding(
            get: { activeLayout },
            set: { newValue in
                guard let appSettings else { return }
                appSettings.dashboardLayout = newValue.rawValue
                appSettings.updatedAt = Date()
                try? modelContext.save()
            }
        )
    }

    private var appearanceBinding: Binding<AppearanceMode> {
        Binding(
            get: { AppearanceMode(rawValue: appSettings?.appearance ?? "system") ?? .system },
            set: { newValue in
                guard let appSettings else { return }
                appSettings.appearance = newValue.rawValue
                appSettings.updatedAt = Date()
                try? modelContext.save()
            }
        )
    }

    // MARK: - Grouped logbook

    private var groupedLogbookBody: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 16) {
                LazyVGrid(columns: AppTheme.compactGrid, spacing: 12) {
                    MetricTile(title: "Operator", value: bridge.dashboard.operatorCallsign, systemImage: "person.crop.circle")
                    MetricTile(title: "Station", value: activeProfile?.stationCallsign ?? bridge.dashboard.activeStation?.stationCallsign ?? "Unset", systemImage: "radio")
                    MetricTile(title: "Pending Uploads", value: "\(pendingUploads)", systemImage: "tray.and.arrow.up", tint: pendingUploads > 0 ? .orange : .green)
                    MetricTile(title: "Network", value: deviceStatus.networkStatus, systemImage: "network", tint: deviceStatus.networkStatus == "Online" ? .green : .orange)
                }

                SectionHeader("Operating Context")
                VStack(spacing: 0) {
                    DetailRow(title: "Profile", value: activeProfile?.displayName ?? bridge.dashboard.currentProfile)
                    DetailRow(title: "Grid", value: dashboardGrid)
                        .accessibilityLabel("Location source \(dashboardGrid)")
                    DetailRow(title: "Location", value: activeProfile?.defaultQTH ?? bridge.dashboard.activeStation?.defaultQth ?? "Unknown")
                    DetailRow(title: "Sync", value: bridge.sync.cloudConnectionState ?? bridge.dashboard.syncStatus?.mode ?? "offline-first")
                    DetailRow(title: "Battery", value: deviceStatus.batteryStatus)
                    DetailRow(title: "Bridge", value: bridge.client.isLive ? "Rust FFI live" : "Rust FFI unavailable")
                }
                .padding()
                .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))

                SectionHeader("Quick Actions")
                LazyVGrid(columns: AppTheme.compactGrid, spacing: 12) {
                    QuickActionButton("New QSO", systemImage: "plus.circle") { selection = .logging }
                    QuickActionButton("Search Callsign", systemImage: "magnifyingglass") { selection = .callsign }
                    QuickActionButton("Start POTA", systemImage: "tree") { selection = .pota }
                    QuickActionButton("Start SOTA", systemImage: "mountain.2") { selection = .sota }
                    QuickActionButton("Open Net", systemImage: "person.3.sequence") { selection = .netControl }
                    QuickActionButton("Sync", systemImage: "arrow.triangle.2.circlepath") { selection = .sync }
                    QuickActionButton("Account", systemImage: "person.crop.circle.badge.checkmark") { selection = .account }
                    QuickActionButton("Admin", systemImage: "person.badge.key") { selection = .admin }
                    QuickActionButton("Open Map", systemImage: "map") { selection = .maps }
                    QuickActionButton("Diagnostics", systemImage: "stethoscope") { selection = .diagnostics }
                }

                SectionHeader("Recent QSOs")
                if qsos.isEmpty {
                    ContentUnavailableView("No QSOs", systemImage: "book.closed", description: Text("Log a contact to populate the dashboard."))
                        .frame(maxWidth: .infinity)
                } else {
                    VStack(spacing: 0) {
                        ForEach(qsos.prefix(6)) { qso in
                            NavigationLink(destination: QSODetailView(qso: qso)) {
                                HStack {
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(qso.callsign)
                                            .font(.headline)
                                        Text("\(qso.qsoKind.capitalized) \(qso.band) \(qso.mode)")
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                    Text(qso.uploadStatus.capitalized)
                                        .font(.caption)
                                        .foregroundStyle(AppTheme.statusColor(qso.uploadStatus))
                                }
                                .padding(.vertical, 8)
                            }
                            Divider()
                        }
                    }
                    .padding(.horizontal)
                    .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
                }

                SectionHeader("Provider Status")
                ProviderSummaryList(providers: bridge.providers.onlineProviders)
            }
            .padding()
        }
    }

    // MARK: - Liquid glass

    /// Session-first: what is happening right now, then the counters, then the
    /// contacts. Nothing here needs a tap to reveal.
    private var liquidGlassBody: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 16) {
                SessionHeroCard(
                    callsign: bridge.dashboard.operatorCallsign,
                    station: activeProfile?.stationCallsign ?? bridge.dashboard.activeStation?.stationCallsign ?? "Unset",
                    grid: dashboardGrid,
                    loggedToday: qsosLoggedToday,
                    pendingUploads: pendingUploads
                )

                LazyVGrid(columns: AppTheme.compactGrid, spacing: 12) {
                    MetricTile(title: "Contacts", value: "\(qsos.count)", systemImage: "antenna.radiowaves.left.and.right")
                    MetricTile(
                        title: "Pending Uploads",
                        value: "\(pendingUploads)",
                        systemImage: "tray.and.arrow.up",
                        tint: pendingUploads > 0 ? .orange : .green
                    )
                    MetricTile(
                        title: "Network",
                        value: deviceStatus.networkStatus,
                        systemImage: "network",
                        tint: deviceStatus.networkStatus == "Online" ? .green : .orange
                    )
                }

                SectionHeader("Recent")
                recentContactsList(limit: 5)

                SectionHeader("Worth Working")
                ProviderSummaryList(providers: bridge.providers.onlineProviders)
            }
            .padding()
        }
    }

    // MARK: - Map and sheet

    /// Map first, with the entry surface pinned underneath so a portable
    /// operator never scrolls to log a contact.
    private var mapSheetBody: some View {
        VStack(spacing: 0) {
            MapContextCard(
                grid: dashboardGrid,
                location: activeProfile?.defaultQTH ?? bridge.dashboard.activeStation?.defaultQth ?? "Unknown",
                contacts: qsos.count
            )
            .frame(maxWidth: .infinity)
            .frame(minHeight: 180)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    QuickActionButton("Log a contact", systemImage: "plus.circle.fill") { selection = .logging }
                        .buttonStyle(.borderedProminent)

                    HStack(spacing: 12) {
                        QuickActionButton("Start POTA", systemImage: "tree") { selection = .pota }
                        QuickActionButton("Spots", systemImage: "dot.radiowaves.left.and.right") { selection = .providers }
                    }

                    SectionHeader("Just logged")
                    recentContactsList(limit: 4)
                }
                .padding()
            }
            .background(.thinMaterial)
        }
    }

    // MARK: - Shared pieces

    @ViewBuilder
    private func recentContactsList(limit: Int) -> some View {
        if qsos.isEmpty {
            ContentUnavailableView("No QSOs", systemImage: "book.closed", description: Text("Log a contact to populate the dashboard."))
                .frame(maxWidth: .infinity)
        } else {
            VStack(spacing: 0) {
                ForEach(qsos.prefix(limit)) { qso in
                    NavigationLink(destination: QSODetailView(qso: qso)) {
                        HStack {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(qso.callsign)
                                    .font(.headline)
                                Text("\(qso.qsoKind.capitalized) \(qso.band) \(qso.mode)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Text(qso.uploadStatus.capitalized)
                                .font(.caption)
                                .foregroundStyle(AppTheme.statusColor(qso.uploadStatus))
                        }
                        .padding(.vertical, 8)
                    }
                    Divider()
                }
            }
            .padding(.horizontal)
            .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
        }
    }

    private var qsosLoggedToday: Int {
        let startOfDay = Calendar.current.startOfDay(for: Date())
        return qsos.filter { $0.contactDate >= startOfDay }.count
    }

    private var pendingUploads: Int {
        let localPending = qsos.filter { $0.uploadStatus != "uploaded" }.count
        return max(localPending, bridge.dashboard.pendingUploads)
    }

    private var dashboardGrid: String {
        if appSettings?.effectiveManualGridOverride == true,
           let manual = HamRadioUtilities.normalizedMaidenhead(appSettings?.maidenheadGrid ?? "") {
            return "\(manual) Manual"
        }
        if appSettings?.effectiveUseDeviceLocation == true,
           let gps = HamRadioUtilities.normalizedMaidenhead(appSettings?.lastGPSGrid ?? "") {
            return "\(gps) GPS"
        }
        if let bridgeGrid = bridge.dashboard.gps?.grid {
            return "\(bridgeGrid) Cached"
        }
        if let stationGrid = activeProfile?.defaultGridSquare, !stationGrid.isEmpty {
            return "\(stationGrid) Station"
        }
        return "Pending"
    }
}

/// The card that answers "what am I in the middle of?" before anything else.
struct SessionHeroCard: View {
    var callsign: String
    var station: String
    var grid: String
    var loggedToday: Int
    var pendingUploads: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("On the air")
                .font(.caption.weight(.bold))
                .textCase(.uppercase)
                .foregroundStyle(.secondary)
            Text("\(loggedToday) today")
                .font(.largeTitle.weight(.bold))
            Text("\(callsign) · \(station) · \(grid)")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            if pendingUploads > 0 {
                Label("\(pendingUploads) waiting to upload", systemImage: "tray.and.arrow.up")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.orange)
            } else {
                Label("Everything uploaded", systemImage: "checkmark.circle")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.green)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 16))
    }
}

/// Stand-in for the full map workspace: enough geography to orient a portable
/// operator without loading the whole map surface into the dashboard.
struct MapContextCard: View {
    var grid: String
    var location: String
    var contacts: Int

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            RoundedRectangle(cornerRadius: 0)
                .fill(.thinMaterial)
                .overlay(
                    Image(systemName: "globe.americas.fill")
                        .font(.system(size: 76))
                        .foregroundStyle(.tertiary)
                )
            VStack(alignment: .leading, spacing: 3) {
                Text(grid)
                    .font(.headline.monospaced())
                Text(location)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("\(contacts) contacts mapped")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .padding()
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Map context: \(grid), \(location), \(contacts) contacts")
    }
}

struct QuickActionButton: View {
    var title: String
    var systemImage: String
    var action: () -> Void

    init(_ title: String, systemImage: String, action: @escaping () -> Void) {
        self.title = title
        self.systemImage = systemImage
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
        }
        .buttonStyle(.bordered)
    }
}

struct SectionHeader: View {
    var title: String

    init(_ title: String) {
        self.title = title
    }

    var body: some View {
        Text(title)
            .font(.headline)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct ProviderSummaryList: View {
    var providers: [ProviderMetadataSnapshot]

    var body: some View {
        if providers.isEmpty {
            Text("Provider metadata is waiting for the Rust bridge.")
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding()
                .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
        } else {
            VStack(spacing: 0) {
                ForEach(providers.prefix(5)) { provider in
                    HStack {
                        Text(provider.displayName)
                        Spacer()
                        Text(provider.requiredCredentials?.isEmpty == false ? "Credentials" : "Ready")
                            .foregroundStyle(provider.requiredCredentials?.isEmpty == false ? .orange : .green)
                    }
                    .padding(.vertical, 8)
                    Divider()
                }
            }
            .padding(.horizontal)
            .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
        }
    }
}

@MainActor
final class DeviceStatusViewModel: ObservableObject {
    @Published var networkStatus = "Unknown"
    @Published var batteryStatus = "Unknown"

    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "KE8YGWLogger.NetworkMonitor")

    func start() {
        #if os(iOS)
        UIDevice.current.isBatteryMonitoringEnabled = true
        updateBattery()
        #endif
        monitor.pathUpdateHandler = { [weak self] path in
            Task { @MainActor in
                self?.networkStatus = path.status == .satisfied ? "Online" : "Offline"
            }
        }
        monitor.start(queue: queue)
    }

    private func updateBattery() {
        #if os(iOS)
        let level = UIDevice.current.batteryLevel
        if level >= 0 {
            batteryStatus = "\(Int(level * 100))%"
        }
        #endif
    }
}
