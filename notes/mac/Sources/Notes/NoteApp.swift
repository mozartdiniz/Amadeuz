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
        .defaultSize(width: 1100, height: 720)
        .commands {
            CommandGroup(after: .appInfo) {
                Divider()
                Button("Sign Out") {
                    vm.logout()
                }
                .disabled(!vm.isAuthenticated)
            }
            CommandGroup(replacing: .newItem) {
                Button("New Note") {
                    vm.createNote()
                }
                .keyboardShortcut("n", modifiers: .command)
                .disabled(!vm.isAuthenticated || vm.selectedFolderID == nil || vm.isTrashView)
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
