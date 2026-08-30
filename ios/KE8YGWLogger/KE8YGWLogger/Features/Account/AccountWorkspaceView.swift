import SwiftUI

/// Hosted account and session surface.
///
/// Rust owns account status, request planning, and response classification.
/// This view renders the Rust snapshot and submits operator intent; it never
/// decides whether a session is valid and never stores token material.
struct AccountWorkspaceView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @StateObject private var service: AccountService
    @State private var serverURL = ""
    @State private var email = ""
    @State private var displayName = ""
    @State private var deviceName = ""
    @State private var invitationToken = ""
    @State private var turnstileToken = ""
    @State private var verificationToken = ""
    @State private var recoveryEmail = ""
    @State private var recoveryToken = ""
    @State private var deleteConfirmation = ""
    @State private var showDeleteConfirmation = false

    init(service: AccountService = AccountService()) {
        _service = StateObject(wrappedValue: service)
    }

    private var snapshot: AccountSessionSnapshot { bridge.account }

    var body: some View {
        List {
            statusSection
            serverSection
            policySection
            if snapshot.isSignedIn {
                sessionSection
                logbookSection
                deviceSection
                deleteSection
            } else {
                signInSection
                registerSection
                verifySection
                recoverySection
            }
        }
        .navigationTitle("Account")
        .disabled(service.isBusy)
        .overlay {
            if service.isBusy {
                ProgressView().controlSize(.large)
            }
        }
        .task {
            service.bind(bridge)
            await service.refresh()
            if serverURL.isEmpty { serverURL = snapshot.serverUrl }
        }
    }

    private var statusSection: some View {
        Section("Status") {
            DetailRow(title: "State", value: snapshot.statusLabel)
            DetailRow(title: "Server", value: snapshot.serverUrl.isEmpty ? "Not configured" : snapshot.serverUrl)
            if let profile = snapshot.account {
                DetailRow(title: "Email", value: profile.email)
                DetailRow(title: "Email Verified", value: profile.isEmailVerified ? "Yes" : "No")
            }
            if let session = snapshot.session, let expiresAt = session.expiresAt {
                DetailRow(title: "Session Expires", value: expiresAt)
            }
            if let device = snapshot.device {
                DetailRow(title: "Device", value: device.deviceName ?? "Unnamed device")
            }
            if let record = snapshot.lastAction {
                VStack(alignment: .leading, spacing: 4) {
                    Text("\(record.kind) / \(record.outcome)")
                        .font(.subheadline)
                    Text(record.message)
                        .font(.caption)
                        .foregroundStyle(record.succeeded ? .secondary : .orange)
                    if let requestId = record.requestId {
                        Text("Request \(requestId)")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            if let error = service.lastError {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
    }

    private var serverSection: some View {
        Section("Hosted Server") {
            TextField("https://logger.example", text: $serverURL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
            Button("Save Server") {
                Task { await service.setServer(serverURL) }
            }
            .disabled(serverURL.trimmingCharacters(in: .whitespaces).isEmpty)
            Text("Cleartext http:// is accepted only for loopback and private self-hosted servers.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var policySection: some View {
        Section("Server Policy") {
            DetailRow(title: "Hosting", value: snapshot.hosting?.operationMode ?? "unknown")
            DetailRow(title: "Registration", value: snapshot.hosting?.registrationMode ?? "unknown")
            if snapshot.hosting?.turnstileRequired == true {
                Text("Public registration on this server requires a Cloudflare Turnstile response.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Button("Read Server Policy") {
                Task { await service.run(.hostingStatus()) }
            }
        }
    }

    private var signInSection: some View {
        Section("Sign In") {
            TextField("Email", text: $email)
                .textContentType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.emailAddress)
            TextField("Display Name", text: $displayName)
            TextField("Device Name", text: $deviceName)
            Button("Sign In") {
                Task {
                    await service.run(
                        .login(
                            email: email,
                            displayName: optional(displayName),
                            deviceName: optional(deviceName)
                        )
                    )
                }
            }
            .disabled(email.trimmingCharacters(in: .whitespaces).isEmpty)
        }
    }

    private var registerSection: some View {
        Section("Create Account") {
            if snapshot.hosting?.registrationDisabled == true {
                Text("Registration is disabled on this server.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if snapshot.hosting?.allowsOpenRegistration != true {
                TextField("Invitation Token", text: $invitationToken)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }
            if snapshot.hosting?.turnstileRequired == true {
                TextField("Turnstile Response", text: $turnstileToken)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }
            Button("Create Account") {
                Task {
                    await service.run(
                        .register(
                            email: email,
                            displayName: optional(displayName),
                            deviceName: optional(deviceName),
                            invitationToken: optional(invitationToken),
                            turnstileToken: optional(turnstileToken)
                        )
                    )
                }
            }
            .disabled(
                email.trimmingCharacters(in: .whitespaces).isEmpty
                    || snapshot.hosting?.registrationDisabled == true
            )
        }
    }

    private var verifySection: some View {
        Section("Verify Email") {
            TextField("Verification Token", text: $verificationToken)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("Verify Email") {
                Task { await service.run(.verifyEmail(token: verificationToken)) }
            }
            .disabled(verificationToken.trimmingCharacters(in: .whitespaces).isEmpty)
        }
    }

    private var recoverySection: some View {
        Section("Account Recovery") {
            TextField("Recovery Email", text: $recoveryEmail)
                .textContentType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.emailAddress)
            Button("Send Recovery Email") {
                Task { await service.run(.recoveryStart(email: recoveryEmail)) }
            }
            .disabled(recoveryEmail.trimmingCharacters(in: .whitespaces).isEmpty)
            TextField("Recovery Token", text: $recoveryToken)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("Complete Recovery") {
                Task {
                    await service.run(
                        .recoveryComplete(token: recoveryToken, deviceName: optional(deviceName))
                    )
                }
            }
            .disabled(recoveryToken.trimmingCharacters(in: .whitespaces).isEmpty)
        }
    }

    private var sessionSection: some View {
        Section("Session") {
            Button("Refresh Session") {
                Task { await service.run(.refreshSession()) }
            }
            Button("Rotate Session") {
                Task { await service.run(.rotateSession()) }
            }
            Button("Sign Out") {
                Task { await service.run(.logout()) }
            }
            Button("Sign Out Everywhere", role: .destructive) {
                Task { await service.run(.logoutAll()) }
            }
        }
    }

    private var logbookSection: some View {
        Section("Logbook Access") {
            let logbooks = snapshot.logbooks ?? []
            if logbooks.isEmpty {
                Text("No hosted logbooks were returned for this account.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(logbooks) { logbook in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(logbook.name ?? "Logbook")
                            .font(.headline)
                        Text(logbook.stationCallsign ?? "No station callsign")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private var deviceSection: some View {
        Section("Devices") {
            Button("Refresh Devices") {
                Task { await service.run(.listDevices()) }
            }
            let devices = snapshot.devices ?? []
            if devices.isEmpty {
                Text("No devices have been read from the hosted server yet.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(devices) { device in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(device.deviceName ?? "Unnamed device")
                                .font(.headline)
                            Text(device.isRevoked ? "Revoked" : "Active")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if !device.isRevoked {
                            Button("Revoke") {
                                Task { await service.run(.revokeDevice(deviceId: device.deviceId)) }
                            }
                            .buttonStyle(.bordered)
                        }
                    }
                }
            }
            Button("Revoke All Devices", role: .destructive) {
                Task { await service.run(.revokeAllDevices()) }
            }
        }
    }

    private var deleteSection: some View {
        Section("Delete Account") {
            Text("Deleting the hosted account revokes every session and device. Local official events stay on this device.")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Type DELETE to confirm", text: $deleteConfirmation)
                .textInputAutocapitalization(.characters)
                .autocorrectionDisabled()
            Button("Delete Hosted Account", role: .destructive) {
                showDeleteConfirmation = true
            }
            .disabled(deleteConfirmation != "DELETE")
            .confirmationDialog(
                "Delete the hosted account?",
                isPresented: $showDeleteConfirmation,
                titleVisibility: .visible
            ) {
                Button("Delete Account", role: .destructive) {
                    Task { await service.run(.deleteAccount()) }
                }
                Button("Cancel", role: .cancel) {}
            }
        }
    }

    private func optional(_ value: String) -> String? {
        let trimmed = value.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? nil : trimmed
    }
}
