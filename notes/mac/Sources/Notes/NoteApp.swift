import AppKit
import SwiftUI

@main
struct NoteApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @StateObject private var vm = NotesViewModel()

    var body: some Scene {
        WindowGroup("Notes") {
            ContentView()
                .environmentObject(vm)
        }
        .defaultSize(width: 1000, height: 680)
        .commands {
            CommandGroup(after: .appInfo) {
                Divider()
                Button("Sign Out") {
                    vm.logout()
                }
                .disabled(!vm.isAuthenticated)
            }
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }
}
