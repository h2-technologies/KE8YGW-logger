import SwiftUI

/// Hosted server administration workspace.
///
/// The view collects operator input and renders the Rust-owned administration
/// snapshot. Rust decides what every hosted response means; Swift only carries
/// the request and reads the session token the account workspace stored in
/// Keychain. Administration targets whichever server the account is signed in
/// to, so there is no separate endpoint setting here.
struct AdminWorkspaceView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @StateObject private var connectivity = ConnectivityMonitor()
    @State private var operationMode = ""
    @State private var registrationMode = ""
    @State private var inviteLogbookId = ""
    @State private var inviteEmail = ""
    @State private var inviteRole = "operator"
    @State private var statusMessage: String?
    @State private var issuedInvitationToken: String?
    @State private var isBusy = false
    @State private var signedIn = false
    @State private var accountEmail: String?
    @State private var pendingRevoke: HostedAdminInvitation?
    private let credentialVault = KeychainCredentialVault()
    private let transport = HostedAdminHTTPTransport()

    private static let operationModes = ["personal_hosted", "public_hosted", "self_hosted"]
    private static let registrationModes = ["invite_only", "open", "disabled"]
    private static let roles = ["viewer", "operator", "admin", "owner"]

    private var admin: HostedAdminSnapshot { bridge.admin }

    var body: some View {
        List {
            Section("Status") {
                DetailRow(title: "Rights", value: rightsLabel)
                DetailRow(title: "Server", value: admin.baseUrl)
                DetailRow(title: "Account", value: accountEmail ?? "not signed in")
                DetailRow(title: "Network", value: connectivity.state.label)
                if !signedIn {
                    Text("Sign in on the Account screen before administering this server.")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
                if admin.administrator == false {
                    Text("The signed-in account is not a server administrator.")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
                if let statusMessage {
                    Text(statusMessage)
                        .font(.caption)
                        .foregroundStyle(admin.lastOutcome == "accepted" ? Color.secondary : Color.orange)
                }
                if isBusy {
                    ProgressView("Contacting the hosted server")
                }
            }

            if let issuedInvitationToken {
                Section("Single-Use Invitation Token") {
                    Text(issuedInvitationToken)
                        .font(.footnote.monospaced())
                        .textSelection(.enabled)
                    Text(
                        "Shown once and never stored. The hosted server also emailed it to the invitee."
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            }

            hostingSection
            invitationsSection
            auditSection
        }
        .navigationTitle("Administration")
        .confirmationDialog(
            "Revoke this invitation?",
            isPresented: Binding(
                get: { pendingRevoke != nil },
                set: { if !$0 { pendingRevoke = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Revoke Invitation", role: .destructive) {
                if let invitation = pendingRevoke {
                    Task { await run(.invitationRevoke(inviteId: invitation.inviteId)) }
                }
                pendingRevoke = nil
            }
            Button("Cancel", role: .cancel) { pendingRevoke = nil }
        }
        .task {
            connectivity.start()
            await loadAdmin()
        }
    }

    private var hostingSection: some View {
        Section("Hosting Configuration") {
            if let hosting = admin.hosting {
                DetailRow(title: "Operation Mode", value: hosting.operationMode ?? "unknown")
                DetailRow(title: "Registration Mode", value: hosting.registrationMode ?? "unknown")
                DetailRow(
                    title: "Bootstrap Admin",
                    value: hosting.bootstrapAdminCompleted ? "completed" : "not completed"
                )
                DetailRow(title: "Session Lifetime", value: seconds(hosting.sessionTtlSeconds))
                DetailRow(title: "Invitation Lifetime", value: seconds(hosting.invitationTtlSeconds))
                if let email = hosting.email {
                    DetailRow(title: "Email Delivery", value: email.mode ?? "unknown")
                }
                if let turnstile = hosting.turnstile {
                    DetailRow(
                        title: "Turnstile",
                        value: turnstile.enabledForOpenRegistration ? "enabled" : "disabled"
                    )
                }
            } else {
                Text("No hosting configuration loaded yet.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Button("Refresh Hosting Configuration") { Task { await run(.hostingRead) } }
                .disabled(isBusy || !signedIn)

            Picker("Operation mode", selection: $operationMode) {
                Text("Leave unchanged").tag("")
                ForEach(Self.operationModes, id: \.self) { mode in
                    Text(label(mode)).tag(mode)
                }
            }
            Picker("Registration mode", selection: $registrationMode) {
                Text("Leave unchanged").tag("")
                ForEach(Self.registrationModes, id: \.self) { mode in
                    Text(label(mode)).tag(mode)
                }
            }
            Button("Update Hosting Configuration") { Task { await updateHosting() } }
                .disabled(isBusy || !signedIn || hostingUpdate.isEmpty)
            Text("Only the fields you change are sent.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var invitationsSection: some View {
        Section("Invitations") {
            if admin.invitations.isEmpty {
                Text("No invitations loaded yet.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            ForEach(admin.invitations) { invitation in
                VStack(alignment: .leading, spacing: 4) {
                    Text(invitation.invitedEmail ?? "unknown")
                        .font(.headline)
                    Text(invitationSummary(invitation))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    HStack {
                        Button("Inspect") {
                            Task { await run(.invitationGet(inviteId: invitation.inviteId)) }
                        }
                        Button("Resend") {
                            Task { await run(.invitationResend(inviteId: invitation.inviteId)) }
                        }
                        .disabled(!invitation.canResend())
                        Button("Expire") {
                            Task { await run(.invitationExpire(inviteId: invitation.inviteId)) }
                        }
                        .disabled(invitation.status() != "pending")
                        Button("Revoke", role: .destructive) { pendingRevoke = invitation }
                            .disabled(invitation.status() != "pending")
                    }
                    .buttonStyle(.bordered)
                    .font(.caption)
                    .disabled(isBusy || !signedIn)
                }
            }

            Button("Refresh Invitations") { Task { await run(.invitationList) } }
                .disabled(isBusy || !signedIn)

            TextField("Logbook ID", text: $inviteLogbookId)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            TextField("Invited email", text: $inviteEmail)
                .keyboardType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Picker("Role", selection: $inviteRole) {
                ForEach(Self.roles, id: \.self) { role in
                    Text(label(role)).tag(role)
                }
            }
            Button("Create Invitation") { Task { await createInvitation() } }
                .disabled(isBusy || !signedIn || !canCreateInvitation)
        }
    }

    private var auditSection: some View {
        Section("Audit Log") {
            if admin.audits.isEmpty {
                Text("No audit records loaded yet.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            ForEach(admin.audits.prefix(50)) { audit in
                VStack(alignment: .leading, spacing: 2) {
                    Text(audit.action ?? "unknown")
                        .font(.subheadline)
                    Text(auditSummary(audit))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Button("Refresh Audit Log") { Task { await run(.auditList) } }
                .disabled(isBusy || !signedIn)
        }
    }

    private var rightsLabel: String {
        switch admin.administrator {
        case .some(true): return "Administrator"
        case .some(false): return "Not an administrator"
        case .none: return "Not checked yet"
        }
    }

    private var hostingUpdate: HostedAdminHostingUpdateRequest {
        HostedAdminHostingUpdateRequest(
            operationMode: optional(operationMode),
            registrationMode: optional(registrationMode)
        )
    }

    private var canCreateInvitation: Bool {
        optional(inviteLogbookId) != nil && optional(inviteEmail) != nil
    }

    private func label(_ value: String) -> String {
        value.replacingOccurrences(of: "_", with: " ").capitalized
    }

    private func seconds(_ value: Int?) -> String {
        guard let value else { return "unset" }
        return "\(value)s"
    }

    private func invitationSummary(_ invitation: HostedAdminInvitation) -> String {
        var parts = [invitation.role ?? "unknown", invitation.status()]
        parts.append("expires \(invitation.expiresAt ?? "never")")
        if invitation.resendCount > 0 {
            parts.append("resent \(invitation.resendCount)x")
        }
        return parts.joined(separator: " / ")
    }

    private func auditSummary(_ audit: HostedAdminAuditRecord) -> String {
        var parts = [audit.outcome ?? "unknown"]
        if let occurredAt = audit.occurredAt { parts.append(occurredAt) }
        if let target = audit.target, !target.isEmpty { parts.append(target) }
        return parts.joined(separator: " / ")
    }

    private func optional(_ value: String) -> String? {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private func loadAdmin() async {
        do {
            let result = try await bridge.refreshHostedAdmin()
            signedIn = result.signedIn
            accountEmail = result.accountEmail
        } catch {
            statusMessage = error.localizedDescription
        }
    }

    private func updateHosting() async {
        let update = hostingUpdate
        guard !update.isEmpty else { return }
        await run(.hostingUpdate(update))
        if admin.lastOutcome == "accepted" {
            operationMode = ""
            registrationMode = ""
        }
    }

    private func createInvitation() async {
        guard let logbookId = optional(inviteLogbookId), let email = optional(inviteEmail) else {
            return
        }
        await run(.invitationCreate(logbookId: logbookId, email: email, role: inviteRole))
        if admin.lastOutcome == "accepted" {
            inviteEmail = ""
        }
    }

    private func run(_ action: HostedAdminActionRequest) async {
        isBusy = true
        defer { isBusy = false }
        // A new request always clears the previously issued one-time token so it
        // is never left on screen next to an unrelated result.
        issuedInvitationToken = nil
        do {
            let execution = try await bridge.executeHostedAdminAction(
                action,
                transport: transport,
                vault: credentialVault,
                networkAvailable: connectivity.state.hasUsableInternet
            )
            statusMessage = execution.result.message
            issuedInvitationToken = execution.invitationToken
        } catch {
            statusMessage = error.localizedDescription
        }
    }
}
