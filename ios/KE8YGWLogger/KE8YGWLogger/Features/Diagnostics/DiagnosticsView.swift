import SwiftData
import SwiftUI
import UniformTypeIdentifiers

struct DiagnosticsView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var qsos: [QSO]
    @State private var reportURL: URL?
    @State private var exportMessage: String?
    @State private var selfTest: BridgeSelfTestResult?
    @State private var selfTestMessage: String?

    var body: some View {
        List {
            Section("Runtime") {
                DetailRow(title: "Rust Version", value: bridge.diagnostics.rustVersion)
                DetailRow(title: "ABI", value: bridge.diagnostics.abiVersion.map(String.init) ?? "unknown")
                DetailRow(title: "Schema", value: bridge.diagnostics.bridgeSchemaVersion.map(String.init) ?? "unknown")
                DetailRow(title: "Sync Protocol", value: bridge.diagnostics.syncProtocolVersion.map(String.init) ?? "unknown")
                DetailRow(title: "Backup Schema", value: bridge.diagnostics.backupSchemaVersion.map(String.init) ?? "unknown")
                DetailRow(title: "Bridge", value: bridge.client.isLive ? "live" : "unavailable")
                DetailRow(title: "Report ID", value: bridge.diagnostics.reportId ?? "")
                DetailRow(title: "Database", value: "SwiftData cache; Rust event store via FFI")
            }

            Section("Queues") {
                DetailRow(title: "Local QSOs", value: "\(qsos.count)")
                DetailRow(title: "Pending Uploads", value: "\(qsos.filter { $0.uploadStatus != "uploaded" }.count)")
                DetailRow(title: "Pending Sync", value: "\(qsos.filter { $0.syncStatus != "synced" }.count)")
            }

            Section("Device") {
                DetailRow(title: "Memory", value: "Provided by iOS diagnostics")
                DetailRow(title: "Storage", value: "Provided by FileManager")
                DetailRow(title: "Crash Info", value: "Provided by platform logs")
                DetailRow(title: "Logs", value: "Runtime JSONL supported by Rust")
            }

            Section("Bridge Self-Test") {
                Button("Run Bridge Self-Test") {
                    Task { await runSelfTest() }
                }
                DetailRow(title: "Result", value: selfTest?.success == true ? "passed" : "not run")
                DetailRow(title: "Library Linked", value: selfTest.map { $0.libraryLinked ? "yes" : "fallback" } ?? "unknown")
                DetailRow(title: "ABI", value: selfTest?.abiVersion.description ?? "unknown")
                DetailRow(title: "Schema", value: selfTest?.bridgeSchemaVersion.description ?? "unknown")
                if let selfTestMessage {
                    Text(selfTestMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Section("Export") {
                Button("Build Diagnostics Report", action: buildReport)
                if let reportURL {
                    ShareLink(item: reportURL) {
                        Label("Share Diagnostics JSON", systemImage: "square.and.arrow.up")
                    }
                }
                if let exportMessage {
                    Text(exportMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("Diagnostics")
        .toolbar {
            Button("Refresh") {
                Task { await bridge.refreshDiagnostics() }
            }
        }
        .task {
            await bridge.refreshDiagnostics()
        }
    }

    private func buildReport() {
        let payload: [String: Any] = [
            "report_id": bridge.diagnostics.reportId ?? UUID().uuidString,
            "rust_version": bridge.diagnostics.rustVersion,
            "abi_version": jsonValue(bridge.diagnostics.abiVersion),
            "bridge_schema_version": jsonValue(bridge.diagnostics.bridgeSchemaVersion),
            "sync_protocol_version": jsonValue(bridge.diagnostics.syncProtocolVersion),
            "backup_schema_version": jsonValue(bridge.diagnostics.backupSchemaVersion),
            "bridge_live": bridge.client.isLive,
            "bridge_self_test": [
                "success": jsonValue(selfTest?.success),
                "library_linked": jsonValue(selfTest?.libraryLinked),
                "abi_version": jsonValue(selfTest?.abiVersion),
                "bridge_schema_version": jsonValue(selfTest?.bridgeSchemaVersion)
            ],
            "qso_count": qsos.count,
            "pending_uploads": qsos.filter { $0.uploadStatus != "uploaded" }.count,
            "pending_sync": qsos.filter { $0.syncStatus != "synced" }.count,
            "generated_at": ISO8601DateFormatter().string(from: Date())
        ]
        do {
            let data = try JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted, .sortedKeys])
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("KE8YGW-Diagnostics.json")
            try data.write(to: url, options: [.atomic])
            reportURL = url
            exportMessage = "Diagnostics report ready."
        } catch {
            exportMessage = error.localizedDescription
        }
    }

    private func runSelfTest() async {
        do {
            selfTest = try await bridge.bridgeSelfTest()
            selfTestMessage = "Self-test completed."
        } catch {
            selfTestMessage = error.localizedDescription
        }
    }

    private func jsonValue<T>(_ value: T?) -> Any {
        value.map { $0 as Any } ?? NSNull()
    }
}

struct BackupRestoreView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query(sort: \QSO.contactDate, order: .forward) private var qsos: [QSO]
    @State private var jsonURL: URL?
    @State private var adifURL: URL?
    @State private var message: String?
    @State private var showingImporter = false

    var body: some View {
        List {
            Section("Backup") {
                DetailRow(title: "QSOs", value: "\(qsos.count)")
                Button("Prepare JSON Backup", action: prepareJSON)
                Button("Prepare ADIF Backup") {
                    Task { await prepareADIF() }
                }
                if let jsonURL {
                    ShareLink(item: jsonURL) {
                        Label("Share JSON", systemImage: "doc")
                    }
                }
                if let adifURL {
                    ShareLink(item: adifURL) {
                        Label("Share ADIF", systemImage: "doc.text")
                    }
                }
            }

            Section("Restore") {
                Text("Use Files or Share Sheet import to bring JSON, ADIF, or ZIP backups into this app.")
                    .foregroundStyle(.secondary)
                Button("Open Import Picker") {
                    showingImporter = true
                }
            }

            if let message {
                Section {
                    Text(message)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("Backup")
        .fileImporter(
            isPresented: $showingImporter,
            allowedContentTypes: [.json, .zip, .data],
            allowsMultipleSelection: false
        ) { result in
            switch result {
            case .success(let urls):
                message = urls.first.map { "Selected \($0.lastPathComponent) for restore." } ?? "No file selected."
            case .failure(let error):
                message = error.localizedDescription
            }
        }
    }

    private func prepareJSON() {
        let rows = qsos.map { qso in
            [
                "callsign": qso.callsign,
                "contact_date": ISO8601DateFormatter().string(from: qso.contactDate),
                "band": qso.band,
                "mode": qso.mode,
                "qso_kind": qso.qsoKind,
                "station_callsign": qso.stationCallsign,
                "operator_callsign": qso.operatorCallsign,
                "grid": qso.gridSquare,
                "notes": qso.notes
            ]
        }
        do {
            let data = try JSONSerialization.data(withJSONObject: rows, options: [.prettyPrinted, .sortedKeys])
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("KE8YGW-Backup.json")
            try data.write(to: url, options: [.atomic])
            jsonURL = url
            message = "JSON backup ready."
        } catch {
            message = error.localizedDescription
        }
    }

    private func prepareADIF() async {
        do {
            let payloads = qsos.map { qso in
                [
                    "qso_id": qso.canonicalID.isEmpty ? qso.id.uuidString : qso.canonicalID,
                    "contacted_callsign": qso.callsign,
                    "station_callsign": qso.stationCallsign,
                    "operator_callsign": qso.operatorCallsign,
                    "started_at": ISO8601DateFormatter().string(from: qso.contactDate),
                    "band": qso.band,
                    "mode": qso.mode,
                    "submode": qso.submode,
                    "frequency_hz": Int(qso.frequencyMHz * 1_000_000),
                    "rst_sent": qso.rstSent,
                    "rst_received": qso.rstReceived,
                    "grid": qso.gridSquare,
                    "name": qso.name,
                    "qth": qso.qth,
                    "notes": qso.notes
                ] as [String: Any]
            }
            let data = try JSONSerialization.data(withJSONObject: payloads)
            let json = String(decoding: data, as: UTF8.self)
            let bridgeExport = try? await bridge.exportADIF(payloads: json)
            let adif = bridgeExport?.adif ?? LogExportService.adif(for: qsos)
            adifURL = try LogExportService.writeTemporaryExportFile(
                name: "KE8YGW-Backup.adi",
                contents: adif
            )
            message = "ADIF backup ready."
        } catch {
            message = error.localizedDescription
        }
    }
}
