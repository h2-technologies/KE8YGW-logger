import SwiftUI

/// Hosted account and session workspace.
///
/// The view collects operator input and renders the Rust-owned account
/// snapshot. Rust decides what every hosted response means; Swift only carries
/// the request and stores the issued session and refresh tokens in Keychain.
struct AccountWorkspaceView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @StateObject private var connectivity = ConnectivityMonitor()
    @State private var serverURL = ""
    @State private var deviceName = ""
    @State private var email = ""
    @State private var displayName = ""
    @State private var invitationToken = ""
    @State private var turnstileToken = ""
    @State private var verificationToken = ""
    @State private var recoveryEmail = ""
    @State private var recoveryToken = ""
    @State private var statusMessage: String?
    @State private var isBusy = false
    @State private var confirmingDeleteAccount = false
    @State private var confirmingRevokeAllDevices = false
    private let credentialVault = KeychainCredentialVault()
    private let transport = HostedAccountHTTPTransport()

    private var account: HostedAccountSnapshot { bridge.account }
    private var isSignedIn: Bool { account.isSignedIn }

    var body: some View {
        List {
            Section("Status") {
                DetailRow(title: "State", value: connectionStateLabel)
                DetailRow(title: "Server", value: account.baseUrl)
                DetailRow(title: "Device", value: account.deviceName)
                DetailRow(title: "Network", value: connectivity.state.label)
                if let email = account.email {
                    DetailRow(title: "Email", value: email)
                }
                if let expiry = account.sessionExpiresAt {
                    DetailRow(title: "Session Expires", value: expiry)
                }
                if let pending = account.pendingEmailVerificationFor {
                    Text("Verification pending for \(pending).")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
                if let statusMessage {
                    Text(statusMessage)
                        .font(.caption)
                        .foregroundStyle(account.lastOutcome == "accepted" ? .secondary : .orange)
                }
                if isBusy {
                    ProgressView("Contacting the hosted server")
                }
            }

            Section("Hosted Server") {
                TextField("Server URL", text: $serverURL)
                    .textContentType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Device name", text: $deviceName)
                    .textInputAutocapitalization(.words)
                Button("Save Server") { Task { await saveServer() } }
                    .disabled(isBusy || serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }

            Section("Sign In") {
                TextField("Email", text: $email)
                    .textContentType(.username)
                    .keyboardType(.emailAddress)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Display name", text: $displayName)
                    .textContentType(.name)
                Button("Sign In") {
                    Task { await run(.login(email: email, displayName: optional(displayName))) }
                }
                .disabled(isBusy || email.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Text("The session token is stored in the iOS Keychain and is never shown or copied into the local cache.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Create Account") {
                TextField("Invitation token", text: $invitationToken)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Turnstile token", text: $turnstileToken)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                Button("Register") {
                    Task {
                        await run(
                            .register(
                                email: email,
                                displayName: optional(displayName),
                                invitationToken: optional(invitationToken),
                                turnstileToken: optional(turnstileToken)
                            )
                        )
                    }
                }
                .disabled(isBusy || email.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                TextField("Email verification token", text: $verificationToken)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                Button("Verify Email") {
                    Task { await run(.verifyEmail(token: verificationToken)) }
                }
                .disabled(isBusy || verificationToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }

            Section("Account Recovery") {
                TextField("Recovery email", text: $recoveryEmail)
                    .keyboardType(.emailAddress)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                Button("Send Recovery Token") {
                    Task { await run(.recoveryStart(email: recoveryEmail)) }
                }
                .disabled(isBusy || recoveryEmail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                TextField("Recovery token", text: $recoveryToken)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                Button("Complete Recovery") {
                    Task { await run(.recoveryComplete(token: recoveryToken)) }
                }
                .disabled(isBusy || recoveryToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }

            Section("Session") {
                Button("Refresh Session") { Task { await run(.session) } }
                    .disabled(isBusy || !isSignedIn)
                Button("Rotate Session") { Task { await run(.sessionRotate) } }
                    .disabled(isBusy || !isSignedIn)
                Button("Sign Out") { Task { await run(.logout) } }
                    .disabled(isBusy || !isSignedIn)
                Button("Sign Out Everywhere", role: .destructive) { Task { await run(.logoutAll) } }
                    .disabled(isBusy || !isSignedIn)
            }

            Section("Logbooks") {
                if account.logbooks.isEmpty {
                    Text("No hosted logbook memberships are cached.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(account.logbooks) { logbook in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(logbook.name).font(.headline)
                            Text([logbook.logbookId, logbook.role].compactMap { $0 }.joined(separator: " / "))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }

            Section("Devices") {
                Button("Refresh Devices") { Task { await run(.deviceList) } }
                    .disabled(isBusy || !isSignedIn)
                if account.devices.isEmpty {
                    Text("No hosted devices loaded yet.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(account.devices) { device in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(device.deviceName).font(.headline)
                            Text(deviceSummary(device))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Button("Revoke", role: .destructive) {
                                Task { await run(.deviceRevoke(deviceId: device.deviceId)) }
                            }
                            .disabled(isBusy || device.revoked || !isSignedIn)
                        }
                    }
                }
                Button("Revoke All Devices", role: .destructive) {
                    confirmingRevokeAllDevices = true
                }
                .disabled(isBusy || !isSignedIn)
            }

            Section("Danger Zone") {
                Button("Delete Hosted Account", role: .destructive) {
                    confirmingDeleteAccount = true
                }
                .disabled(isBusy || !isSignedIn)
                Text("Deleting the hosted account does not delete the local official log.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Account")
        .confirmationDialog(
            "Revoke every hosted device?",
            isPresented: $confirmingRevokeAllDevices,
            titleVisibility: .visible
        ) {
            Button("Revoke All Devices", role: .destructive) {
                Task { await run(.deviceRevokeAll) }
            }
            Button("Cancel", role: .cancel) {}
        }
        .confirmationDialog(
            "Delete the hosted account?",
            isPresented: $confirmingDeleteAccount,
            titleVisibility: .visible
        ) {
            Button("Delete Hosted Account", role: .destructive) {
                Task { await run(.accountDelete) }
            }
            Button("Cancel", role: .cancel) {}
        }
        .task {
            connectivity.start()
            await loadAccount()
        }
    }

    private var connectionStateLabel: String {
        account.connectionState.replacingOccurrences(of: "_", with: " ").capitalized
    }

    private func deviceSummary(_ device: HostedAccountDevice) -> String {
        var parts: [String] = [device.deviceId]
        if device.current { parts.append("This device") }
        parts.append(device.revoked ? "Revoked" : "Active")
        if device.trusted { parts.append("Trusted") }
        return parts.joined(separator: " / ")
    }

    private func optional(_ value: String) -> String? {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private func loadAccount() async {
        do {
            let snapshot = try await bridge.refreshHostedAccount()
            if serverURL.isEmpty { serverURL = snapshot.baseUrl }
            if deviceName.isEmpty { deviceName = snapshot.deviceName }
            if email.isEmpty, let stored = snapshot.email { email = stored }
        } catch {
            statusMessage = error.localizedDescription
        }
    }

    private func saveServer() async {
        isBusy = true
        defer { isBusy = false }
        do {
            let snapshot = try await bridge.configureHostedAccount(
                baseURL: serverURL,
                deviceName: optional(deviceName)
            )
            serverURL = snapshot.baseUrl
            deviceName = snapshot.deviceName
            statusMessage = "Hosted server saved."
        } catch {
            statusMessage = error.localizedDescription
        }
    }

    private func run(_ action: HostedAccountActionRequest) async {
        isBusy = true
        defer { isBusy = false }
        do {
            let result = try await bridge.executeHostedAccountAction(
                action,
                transport: transport,
                vault: credentialVault,
                networkAvailable: connectivity.state.hasUsableInternet
            )
            statusMessage = result.message
            if result.isAccepted {
                verificationToken = ""
                recoveryToken = ""
                invitationToken = ""
                turnstileToken = ""
            }
        } catch {
            statusMessage = error.localizedDescription
        }
    }
}
