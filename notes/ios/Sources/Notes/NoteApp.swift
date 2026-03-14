import SwiftUI

@main
struct NoteApp: App {
    @StateObject private var vm = NotesViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(vm)
        }
    }
}
