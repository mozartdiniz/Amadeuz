import SwiftUI

struct AuthView: View {
    @ObservedObject var vm: NotesViewModel
    @State private var mode: Mode = .login
    @State private var email        = ""
    @State private var password     = ""
    @State private var recoveryCode = ""
    @State private var newPassword  = ""
    @State private var isLoading = false

    enum Mode: String, CaseIterable {
        case login    = "Sign In"
        case register = "Register"
        case recover  = "Recover"
    }

    var body: some View {
        VStack(spacing: 24) {
            Text("Amadeuz")
                .font(.largeTitle.bold())

            Picker("", selection: $mode) {
                ForEach(Mode.allCases, id: \.self) { m in
                    Text(m.rawValue).tag(m)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .onChange(of: mode) { _, _ in vm.authError = nil }

            VStack(spacing: 10) {
                TextField("Email", text: $email)
                    .textFieldStyle(.roundedBorder)

                if mode == .recover {
                    SecureField("Recovery Code", text: $recoveryCode)
                        .textFieldStyle(.roundedBorder)
                    SecureField("New Password", text: $newPassword)
                        .textFieldStyle(.roundedBorder)
                } else {
                    SecureField("Password", text: $password)
                        .textFieldStyle(.roundedBorder)
                }
            }

            if let err = vm.authError {
                Text(err)
                    .foregroundStyle(.red)
                    .font(.caption)
                    .multilineTextAlignment(.center)
            }

            Button(action: submit) {
                Group {
                    if isLoading {
                        ProgressView()
                    } else {
                        Text(mode.rawValue)
                            .frame(maxWidth: .infinity)
                    }
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(isLoading || !formIsValid)
            .keyboardShortcut(.defaultAction)
        }
        .padding(32)
        .frame(width: 320)
    }

    private var formIsValid: Bool {
        guard !email.isEmpty else { return false }
        switch mode {
        case .login, .register: return !password.isEmpty
        case .recover:          return !recoveryCode.isEmpty && !newPassword.isEmpty
        }
    }

    private func submit() {
        isLoading = true
        Task {
            switch mode {
            case .login:    await vm.login(email: email, password: password)
            case .register: await vm.register(email: email, password: password)
            case .recover:  await vm.recover(email: email, code: recoveryCode, newPassword: newPassword)
            }
            isLoading = false
        }
    }
}

// MARK: - Recovery code sheet

struct RecoveryCodePresentation: Identifiable {
    let id   = UUID()
    let code: String
}

struct RecoveryCodeView: View {
    let code: String
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 20) {
            Image(systemName: "key.fill")
                .font(.system(size: 40))
                .foregroundStyle(.orange)

            Text("Save Your Recovery Code")
                .font(.headline)

            Text("This code lets you recover your account if you forget your password. Store it somewhere safe — it won't be shown again.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            Text(code)
                .font(.system(.body, design: .monospaced))
                .padding(12)
                .background(.quaternary, in: RoundedRectangle(cornerRadius: 8))

            Button("Copy") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(code, forType: .string)
            }
            .buttonStyle(.bordered)

            Button("Done") { dismiss() }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
        }
        .padding(32)
        .frame(width: 380)
    }
}
