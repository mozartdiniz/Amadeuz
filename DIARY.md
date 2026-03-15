# Amadeuz — Build Diary

This file is a running journal of the technical decisions, conversations, doubts, and
discoveries made while building Amadeuz. It is written as it happens — not cleaned up
after the fact.

The goal is to turn these entries into blog posts: one per milestone, telling the real story
of building a self-hosted, cross-platform sync app assisted by Claude Code. The audience is
developers of all levels, including people who are still learning.

**How to read this file:**
Technical reasoning is written in plain paragraphs. Personal reflections are in blockquotes
and marked with `📝` — those sections are notes-to-self to expand when writing the actual
post.

---

## March 8, 2026 — The idea, the server, and the first working sync

### What I wanted to build

> 📝 *Write here: what was the moment that made you want to build this? What specifically
> frustrated you about iCloud / existing sync solutions? How long had this idea been in
> your head before you actually started?*

The vision is simple to state and hard to execute: an open-source iCloud replacement.
Not another Nextcloud clone with a web UI. Something that feels genuinely native on
every platform — like Apple built it, but you own it.

The first decision was to start with **Notes** specifically, not files or photos.
A note is just a string. There is no binary format, no thumbnails, no metadata complexity.
If the sync protocol works for a string, it will work for everything else later.
Start with the smallest useful thing that proves the whole idea.

### The architecture decision: native everywhere, no Electron

This was not a hard decision but it is worth stating clearly because it shapes everything
that comes after.

The tempting shortcut is to write a web app and wrap it in Electron or Tauri.
One codebase, every platform. But you can always tell. The fonts are slightly off.
The scrolling doesn't have momentum. The menu bar doesn't integrate. The window shadow
is wrong. Native apps have a feel that web wrappers cannot replicate, and that feel is
precisely what makes iCloud apps pleasant to use.

So the rule became: each platform gets its own client in its own native language and
framework, even if that means writing the same logic four times.

| Platform | Chosen stack |
|----------|-------------|
| Server   | Go |
| macOS    | Swift + SwiftUI |
| Windows  | C# + WinUI 3 |
| Linux    | C++ + GTK4 |
| iOS      | Swift + SwiftUI (shares logic with macOS) |

The server and each client are separate codebases that share nothing but a JSON protocol.
That is a feature, not a bug. Each one can evolve at its own pace.

### Building the server first

The server was the right place to start. Before writing any UI, I needed to know that the
sync idea was actually correct.

**Why Go?** A few reasons. Go compiles to a single static binary — no runtime to install,
no dependency management on the server side. Its concurrency model (goroutines + channels)
is a natural fit for a WebSocket hub that needs to track many live connections.
And the standard library is extremely capable; the only external dependency needed was
`gorilla/websocket`, which is the de-facto standard for Go WebSocket work.

**Why WebSockets and not something else?** The two realistic alternatives were HTTP polling
(simple but wastes bandwidth and adds latency) and Server-Sent Events (good for
server-to-client push, but not bidirectional). WebSockets give a persistent, full-duplex
channel: the server can push to clients instantly when another client updates, and clients
can push updates back without the overhead of a new HTTP connection every time.

**The sync protocol: last-write-wins with timestamps**

The first real design decision was how to resolve conflicts. The fancy answer is CRDTs or
operational transforms. The honest answer for a prototype is: whoever wrote last wins.

Every message carries a Unix millisecond timestamp called `updated_at`. When two clients
are both online and one sends an update, the server only accepts it if the timestamp is
strictly newer than what it has stored. When a client reconnects after being offline, it
compares its local timestamp with the server's and pushes its version if it is ahead.
This is not perfect — if two people edit simultaneously, one edit will be silently lost.
But for a single-user notes app across your own devices, this never actually happens.
Ship the simple version first; add CRDTs later when the problem is proven.

**The JSON format decision: camelCase locally, snake_case on the wire**

This one is a minor detail that matters for consistency across platforms.

On the wire (server protocol): `updated_at` — snake_case, conventional for Go and JSON APIs.
In local files (stored per device): `updatedAt` — camelCase, conventional for Swift/JSON.

This asymmetry came from the macOS client being written first. Swift's `Codable` naturally
uses camelCase. Rather than fight it, the convention was made explicit: wire format is
snake_case, local files are camelCase. Every client follows this. It is documented in
`VISION.md` so future clients don't accidentally diverge.

### Building the macOS app alongside the server

The server and macOS client landed in the same first commit. That was intentional.
There is no point shipping a server you cannot test, and there is no point building a client
with no server to connect to. The end-to-end flow had to work before anything else mattered.

**Why SwiftUI and not AppKit?**

> 📝 *Write here: do you have a history with AppKit? Was SwiftUI a relief, a frustration,
> or both? How did it feel to see the text editor update in real time across two windows?*

SwiftUI is Apple's current framework. AppKit is the underlying layer it sits on, and it
still leaks through in places — this project hit one of them immediately.

SPM (Swift Package Manager) executables don't activate as foreground apps by default.
When you run `swift run`, the window opens but sits behind whatever else is on screen.
The fix is to drop down to AppKit and add an `AppDelegate` that calls
`NSApp.setActivationPolicy(.regular)` and `NSApp.activate(ignoringOtherApps: true)`.
Two lines. But you have to know they exist, and they feel out of place inside a SwiftUI app.
This is the kind of thing that makes working with Apple's frameworks interesting — and
occasionally maddening.

**The offline-first model**

Every client loads from local disk immediately on launch. The app is fully functional before
the WebSocket connection is established, and fully functional if the connection never happens.
The server is optional.

This changes the feel of the app significantly. There is no loading state. There is no
spinner. You open it and your note is there. Connectivity is a background concern.

**The 500ms debounce**

The note editor does not send to the server on every keystroke. It waits 500 milliseconds
after the last keystroke before saving locally and pushing to the server. This is called
debouncing.

Why 500ms specifically? It is a guess that feels right. Fast enough that a pause in typing
triggers a save. Long enough that rapid typing does not send dozens of network messages per
second. The same value is used on every platform.

There is also an echo guard: when the server sends an update, the client stores it as
`lastReceivedContent`. The debounce checks whether the current content matches this value
before sending — if it does, it skips the send. Without this, every update received from
the server would trigger a send back to the server, which would trigger the same on every
other client, and you would have an infinite loop.

> 📝 *Write here: what was the moment the two-window live sync worked for the first time?
> Where were you, what did it feel like to see text appear in real time?*

---

## March 8, 2026 (afternoon) — Windows: the C++ plan that became C#

### The spec said C++. The implementation chose C#.

The original plan, written in `CLAUDE.md`, was C++ + WinUI 3.
The actual code is C# + WinUI 3.

This is worth explaining because it is a real decision with real tradeoffs, not a mistake.

WinUI 3 is Microsoft's current native UI framework, the successor to UWP, and the right
choice for a modern Windows app. It works with both C++ and C#. The difference is in
the ergonomics.

In C++, the WinRT APIs that back WinUI 3 are accessible via C++/WinRT — a header-only
projection that makes WinRT types usable in modern C++. It works, but the template-heavy
syntax is verbose, the async model is unfamiliar, and the tooling is less forgiving.
You end up writing a lot of boilerplate that has nothing to do with the problem.

In C#, the same WinRT APIs are exposed through clean .NET wrappers. Async/await reads
like English. The type system guides you. The Visual Studio tooling is first-class.

For a component that needs to be a native Windows app with modern UX, C# is simply the
path of least resistance. The end result is the same binary, the same capabilities,
the same native feel. The decision was made while actually sitting down to build it:
the C++ path was harder without being better for the user.

> 📝 *Write here: when did you make the call to switch? Was it after trying C++ and
> hitting friction, or was it clear before you started? How do you feel about having
> the spec and the implementation diverge on day one?*

### Mica and caring about native feel

The Windows app uses Mica — a Windows 11 design material that creates a translucent
backdrop tinted by the user's wallpaper and accent colour. It is one line:
`SystemBackdrop = new MicaBackdrop()`.

This is a small thing but an important one. Mica is what separates a Windows app that
*runs on Windows* from a Windows app that *feels like Windows*. A notes app in particular
spends a lot of time just sitting on screen. It should look like it belongs there.

The same philosophy drove the font choice: `Segoe UI Variable Text`, the native Windows 11
reading typeface. These details are invisible when they are right and jarring when they are
wrong.

### Threading on Windows vs macOS

On macOS, all the WebSocket callbacks are dispatched via `URLSession` and need to be
marshaled back to the main thread with `DispatchQueue.main.async` before touching UI.
On Windows, `ClientWebSocket` runs on background threads, so the callbacks in `SyncService`
fire on background threads. The view model must dispatch back to the UI thread using
`DispatcherQueue.TryEnqueue`.

This is the same problem expressed differently by each platform's async model.
The lesson is that any cross-platform sync layer will hide this detail inside the
view model — the UI code should never have to think about which thread an update arrives on.

> 📝 *Write here: what was the experience of building on Windows? Do you normally
> develop on Mac? Was it strange context-switching between the two? Was Visual Studio
> comfortable or foreign?*

---

## March 9, 2026 — Linux: GTK4, an accidental git commit, and writing it all down

### Why GTK4

Linux desktop development has more options than the other platforms and fewer obvious
answers. The main contenders were:

- **Qt** — cross-platform, C++, mature, excellent. But Qt has its own look-and-feel
  that sits on top of the platform rather than integrating with it. On GNOME it looks
  slightly foreign. On KDE it is the native toolkit, so Qt would be the right answer
  on a KDE-first app. The goal here is GNOME-first.

- **GTK4** — the native GNOME toolkit. C-based, used by GNOME itself, Nautilus,
  gedit, and most of the GNOME app ecosystem. The right choice if the goal is a GNOME
  app that feels like a GNOME app.

- **Electron / Tauri** — ruled out on principle. See the native-everywhere rule above.

- **Flutter** — interesting but adds a rendering engine that bypasses the platform widget
  set entirely. The same objection as Electron, just with a different renderer.

GTK4 was the answer. C++ was chosen over pure C because the other clients use
object-oriented patterns (classes, RAII, destructors) that map naturally, and because
C++ gives `std::string`, `std::unique_ptr`, and lambdas — all of which make the code
significantly cleaner.

### The WebSocket question on Linux

On macOS, `URLSessionWebSocketTask` is in the standard library. On Windows,
`ClientWebSocket` is in the .NET runtime. On Linux, there is no single blessed WebSocket
implementation — you choose a library.

The options discussed:

- **libwebsockets** — purpose-built, fast, well-maintained. But it has its own event loop
  that needs to be integrated with GLib's event loop. That integration is non-trivial.

- **Boost.Beast** — excellent C++ WebSocket implementation from the Boost networking library.
  But it brings Boost as a dependency, which is heavy, and again has its own async model
  (Asio) that needs bridging to GLib.

- **libsoup-3** — GNOME's HTTP library. Less obvious as a WebSocket choice, but it has
  full WebSocket support via `SoupWebsocketConnection`, and its async model *is* GLib's
  async model. There is no bridging needed. Everything runs on the same main loop that
  GTK4 already uses.

libsoup-3 was the right answer specifically because of that integration. On Linux with GTK,
all callbacks — UI events, network events, timers — are dispatched by the GLib main loop on
the main thread. This means the GTK4 client has no explicit thread marshaling at all.
No `DispatchQueue.main.async`, no `DispatcherQueue.TryEnqueue`. The server message arrives,
the callback fires, the GTK widgets update. This is actually the simplest threading model
of the three platforms.

### The debounce on GLib

On macOS the debounce uses Combine's `.debounce(for:scheduler:)` operator — declarative,
one line. On Windows it uses `CancellationTokenSource` with `Task.Delay` — more explicit
but idiomatic C#. On Linux it uses `g_timeout_add` and `g_source_remove` — GLib's timer
mechanism.

The pattern in all three cases is the same: on each keystroke, cancel any pending timer and
start a new 500ms one. When the timer fires, flush. The mechanism looks different in each
language but the logic is identical, which is a good sign that the design is right.

### The accidental git commit

Before the Linux code was pushed, `git status` revealed that the entire `build/` directory
had been staged — compiled object files, the final binary, CMake's internal bookkeeping,
all of it. About 35 files that had no business being in version control.

This is a common mistake with CMake projects. CMake generates its build directory alongside
the source by default (or in a `build/` subdirectory you create). Nothing stops it from
being `git add`ed, and nothing will warn you unless you have a `.gitignore` in place.

The fix: create `notes/linux/.gitignore` with `build/` as a single entry, then
`git rm -r --cached notes/linux/build/` to remove the already-staged files from the index
without deleting them from disk. The build directory stays, just invisible to git.

> 📝 *Write here: is this the first time you made this kind of mistake? Do you have a
> habit of checking `git status` carefully or do you tend to `git add .` and trust it?
> What does it feel like to catch something like this before it lands in a commit?*

This kind of moment is worth documenting honestly. Experienced developers make this mistake.
The `.gitignore` file exists precisely because the mistake is universal. If you are learning
git and you have done this — you are not alone, and the fix is always straightforward.

### Stopping to write it all down

After the Linux client was working, the session shifted to documentation. Three files:
`VISION.md`, `PROGRESS.md`, and this one.

`VISION.md` is the project's long-term document — principles, architecture, roadmap, and a
decisions log. The decisions log matters especially: it is easy to look at a codebase and
see *what* was decided. It is very hard to reconstruct *why*. Writing the why down while
the reasoning is fresh is the most valuable thing you can do for your future self (and for
anyone else who reads the code).

`PROGRESS.md` is a practical tool for working across machines with Claude Code. The idea is
that you open a new session, hand Claude that file, and it immediately knows the state of
every platform. No re-explaining the architecture from scratch. The feature matrix — a table
of features vs platforms — makes parity gaps visible at a glance.

`DIARY.md` (this file) is the raw material for the blog posts. The distinction matters:
`VISION.md` is a polished reference document. This is unpolished honesty.

> 📝 *Write here: when did you decide to document the process publicly? Was it always the
> plan to write about it, or did it occur to you partway through? Who are you writing for —
> your past self? Junior developers? The open-source community? All of the above?*

### Where things stand at the end of day two

Three clients are working. The server is running. If you open the macOS app on one machine,
the Windows app on another, and the Linux app on a third, and point all three at the same
server, text typed on any one of them will appear on the others within a second.

That is the core loop. Everything that comes next — multiple notes, user accounts, files,
encryption — is built on top of this. The foundation is solid.

What is missing before calling this a real POC:

- The iOS client (shares most code with macOS — should be the fastest to build)
- An actual end-to-end test with all platforms running simultaneously on a LAN
- Packaging (so it can be installed normally, not just run from the terminal)

> 📝 *Write here: how does it feel to have a working prototype across three platforms in
> two days? Did it go faster or slower than expected? What surprised you the most —
> about the technology, about the process, or about working with Claude Code?*

---

<!-- ─────────────────────────────────────────────
     FUTURE ENTRIES GO BELOW THIS LINE
     Format: ## [Date] — [Milestone title]
     ───────────────────────────────────────────── -->

---

## March 9, 2026 (night) — Folders, multiple notes, and the first real tests

### Skipping Phase 2a

The architecture doc said to implement user accounts first. We skipped it and went straight
to multiple notes and folders.

The reason is practical: building auth before the core data model is validated means building
auth for a schema that might change. Notes and folders are the product. Auth is plumbing.
Get the data model working end-to-end first, then layer auth on top. A working notes app
without logins is more useful to test than a login screen with nowhere to go.

> 📝 *Write here: does it feel weird to deviate from the plan you wrote a few hours ago?
> Is that flexibility or indiscipline? How do you tell the difference?*

### What the UI reference told us about the data model

The starting point was a screenshot of Apple Notes. Three columns: folder list on the left,
note list in the middle, note content on the right.

This immediately told us what data we needed:

- `Folder`: `id`, `name`, `created_at`
- `Note`: `id`, `folder_id`, `title`, `content`, `updated_at`, `created_at`

And what operations the server needs to support: create/rename/delete folder, create/update/delete
note, cascade delete notes when a folder is deleted. The screenshot was a better spec than
a written requirements list would have been.

### Rewriting the server

The original server was 155 lines in a single `main.go`. It handled one thing: a single
shared note. The new server is split into four files:

- `model.go` — data types and the wire message format
- `store.go` — all in-memory state and disk persistence, with a clear interface
- `hub.go` — WebSocket hub and message routing
- `main.go` — HTTP setup only

The separation matters. `store.go` has no WebSocket dependency. It can be tested with plain
Go tests, no network required. `hub.go` has no persistence logic — it just calls the store
and broadcasts results. Each file has one job.

### The wire protocol grew up

The original protocol had two message types: `init` and `update`. The new protocol has twelve.
Every mutation (create, rename, delete for folders; create, update, delete for notes) has its
own message type. This is more code but it eliminates ambiguity — the type field tells you
exactly what happened without having to infer it from payload structure.

One subtlety about `create_*` vs `update_*` and `delete_*`: create responses are sent to
the originating client *and* broadcast to others. The reason is that the client needs the
server-assigned ID. You send `create_note` with a title; the server responds with
`note_created` containing the new UUID. Without that round-trip, the client cannot reference
the note for future updates.

Update and delete are the opposite: the originating client already knows what changed (it
initiated the change), so the broadcast goes only to other clients. The server acts as a
relay, not an acknowledgement system, for those operations.

### The persist() race condition

The original code called `go s.persist()` — a goroutine — every time the store changed.
Simple and apparently correct. It is actually a race condition.

Consider two goroutines: goroutine A was spawned after a `createFolder` call, goroutine B
after a subsequent `createNote` call. Both take a snapshot of the store's current state,
then write to disk. If goroutine A runs *after* goroutine B, it overwrites the file with
an older snapshot that doesn't include the note. The client will reconnect and find a folder
with no notes in it.

The fix has two parts:
1. Synchronous persist — no goroutines. The mutation waits for the write to finish.
2. Atomic write — write to a temp file, then `os.Rename`. Rename is atomic on POSIX
   filesystems. If the process is killed mid-write, the original file is intact.

For a notes app, synchronous writes are fine. A debounced keystroke happens at most twice
a second; a local file write takes microseconds. The complexity of goroutine coordination
is not worth the negligible performance difference.

### The tests

20 unit tests for `store.go`. They cover:

- Creating, renaming, and deleting folders — including not-found cases
- Cascade delete: removing a folder deletes its notes but not notes in other folders
- Creating notes in valid and invalid folders
- The last-write-wins logic: updates with older or equal timestamps are rejected
- Persist and reload: write state, create a new store from the same file, verify everything survived
- Atomic write format: the file on disk is valid JSON with the right structure
- ID uniqueness: two notes created in the same folder get different IDs

The `TestPersistAndReload` test caught the race condition described above — it was failing
because async goroutines were overwriting the file after the synchronous `persist()` call
in the test, leaving the file truncated or empty on reload. Making persist synchronous made
the test deterministic.

Tests running against the store in isolation, with no WebSocket machinery involved, meant
the feedback loop was fast. Change something in `store.go`, run `go test ./...`, see results
in under a second. This is what separates testable code from code you have to run end-to-end
to verify.

> 📝 *Write here: do you normally write tests for side projects like this? Does the
> presence of tests change how you feel about the code — more confident, more constrained,
> or both?*

### Where things stand

The server now supports folders and multiple notes. The wire protocol is documented.
The store is tested. The clients (macOS, Windows, Linux) are still on the old single-note
protocol — they will fail to compile against the new one. That is intentional: the server
moves first, then each client is updated to match.

Next session: update the macOS client. The UI changes from a single `TextEditor` to a
three-column layout: folders, note list, note editor. The view models split accordingly.
The sync logic changes from "sync one note" to "sync a collection."

---

## March 9, 2026 (late) — Getting the Windows app to run outside Visual Studio

### The problem with "just double-click it"

After the Windows app was working in Visual Studio, there was one thing left to do before
calling the POC complete: make it run like a normal app. Not from the IDE. Not with a
developer environment set up. Just: copy a folder somewhere, double-click the exe, it opens.

This turned out to be the hardest part of the entire Windows client.

### First attempt: dotnet publish

The natural tool for publishing a .NET app is `dotnet publish`. Run it, point it at the
project, get a self-contained folder. Except WinUI 3 is not a normal .NET app — it uses the
Windows App SDK, which has a resource compilation step (`ExpandPriContent`) that requires
tools installed by Visual Studio, not the dotnet SDK. `dotnet publish` exits with an error
about a missing DLL somewhere in the VS install path.

The fix: use `MSBuild.exe` from Visual Studio directly.
The lesson: WinUI 3 builds are Visual Studio builds, not dotnet builds. The toolchain is
different in ways that are easy to miss until you hit them.

### The silent crash

First successful publish: 354 files, 163 MB. Double-click. Nothing happens. No window,
no error message, nothing.

The Windows Event Log had the answer: `STATUS_FAIL_FAST_EXCEPTION` from `Microsoft.UI.Xaml.dll`.
The XAML framework was calling `RaiseFailFastException` — a fast-kill that generates no
dialog and no output — because the Windows App Runtime had never been initialized.

WinUI 3 unpackaged apps require the Windows App Runtime to be bootstrapped before XAML can
activate any WinRT classes. There is a mechanism for this — `MddBootstrapAutoInitializer.cs`,
a C# file included by the WinAppSDK build targets that uses a `[ModuleInitializer]` to run
bootstrap initialization before `Main`. But it is disabled by a condition in
`BootstrapCommon.targets`:

```xml
Condition="... and '$(WindowsAppSDKSelfContained)'!='true' ...">
    <WindowsAppSdkBootstrapInitialize>true</WindowsAppSdkBootstrapInitialize>
```

Because `WindowsAppSDKSelfContained=true` was set in the project, the auto-init never ran.
The fix: `<WindowsAppSdkBootstrapInitialize>true</WindowsAppSdkBootstrapInitialize>` in the
csproj, which forces the auto-initializer on regardless of the self-contained setting.

### The version mismatch

With bootstrap initialization enabled, the app finally showed some life: a dialog asking
to install Windows App Runtime 1.6. The installed runtime was already 1.6 — or so it seemed.

The machine is running Windows Insider Preview. It has a CBS (Component Based Servicing)
version of the Windows App Runtime pre-installed as a Windows component. The bootstrapper
from the NuGet package looks for the Store-distributed package by a specific package family
name. The CBS package has a different name and is invisible to it.

Upgrading to WinAppSDK 1.8 (the current stable release) changed the error: the dialog
disappeared, but the app crashed immediately in `CoreMessagingXP.dll` with `STATUS_NOT_SUPPORTED`.
The self-contained publish had bundled `CoreMessagingXP.dll` version 10.0.27108 — a DLL built
against a Windows build newer than the one on this machine (26200). The bundled DLL was
calling an API that did not exist yet on this OS version.

### The actual fix

Remove `WindowsAppSDKSelfContained=true`. Stop bundling the WinAppSDK native DLLs entirely.
Let the bootstrapper find and use the Windows App Runtime that is installed on the system.

The app is still self-contained for .NET (the .NET 9 runtime is included in the publish
folder). Only the WinAppSDK native layer comes from the system install. The installed
non-CBS `Microsoft.WindowsAppRuntime.1.8` (version 8000.770.947.0) was found and loaded.

One remaining issue: without self-contained WinAppSDK, the PRI resource files — the Windows
binary format for app resources — were no longer copied to the publish output. XAML needs
these to resolve `ms-appx:///` URIs at startup. The crash was deep inside
`Microsoft.UI.Xaml.dll` this time. The fix: copy them manually from the build output,
then add a custom MSBuild target (`CopyPriFilesToPublish`) to the csproj so future publishes
include them automatically.

After that: it worked.

> 📝 *Write here: how did it feel to spend this much time on packaging — a problem that
> has nothing to do with the app's actual functionality? Is this kind of platform friction
> frustrating, or is debugging it satisfying in its own way? What does it say about the
> state of Windows development tooling that this isn't straightforward?*

### POC complete

With the Windows app distributable as a folder, the Phase 1 POC is done. Three native
clients — macOS, Windows, Linux — all syncing to a server running on a Raspberry Pi over
LAN. Each client built in the idiomatic language and framework for its platform, with no
shared code except a JSON protocol.

The next phase begins: user accounts, multiple notes, end-to-end encryption, and sharing.
The architecture for all of this was already designed in the previous session.
The foundation is ready. Time to build on it.

---

## March 9, 2026 (evening) — The hard questions: users, encryption, sharing

### Why this conversation happened now

Three native clients. One server. Text syncs over LAN in real time. The proof of concept
works. The next natural question is: what does this become?

The immediate answer was: users, encryption, folders, sharing. But before writing any code,
it was worth stepping back and asking the architecture question that would otherwise haunt
every subsequent decision: should this be one server or multiple?

### The microservices temptation

The instinct, when you imagine a system that will eventually handle notes, files, contacts,
reminders, and calendar — all as separate features — is to reach for microservices.
One service per feature. Independent deployments. Independent scaling.

But this project's constraint reverses all the usual tradeoffs. Microservices are designed
for teams at scale: multiple developers deploying independently, services with different
resource profiles, failure isolation across many users. None of that applies here.
The operator is a single person running a server at home.

For self-hosting, microservices are a liability. You multiply the number of processes to
manage, ports to expose, and things that can silently fail. The operator who just wants
their notes to sync should not have to run a Docker Compose file with five services to do it.

**Decision: modular monolith.** One binary, one database file, one config. Features are
internal Go packages. Adding a new feature domain (files, contacts) means adding a new
package, not a new service. The package boundaries enforce the same separation of concerns
that microservices would, without any of the operational complexity.

The one exception made: feature flags in the config file. An operator who does not want
the reminders feature running can turn it off without rebuilding anything.

### Choosing SQLite

The current server stores its state in a JSON file. That was fine for one shared note.
It is obviously wrong for multiple notes owned by different users with access control.

SQLite was the right replacement. The less obvious choice was *which* SQLite driver.
The standard Go SQLite driver (`mattn/go-sqlite3`) requires CGO — a C compiler.
That means cross-compilation to ARM (for Raspberry Pi) requires a full cross-compilation
toolchain, which is a significant setup burden.

`modernc.org/sqlite` is a pure-Go port of SQLite. No CGO. Cross-compile with
`GOOS=linux GOARCH=arm64 go build .` and it just works. For a project whose target
deployment is a Raspberry Pi on someone's home network, this matters.

### The E2E encryption design

This is the genuinely hard part. "End-to-end encryption" is a phrase that is easy to say
and difficult to implement correctly. The critical constraint: the server must be unable to
read note content. Even if the database is stolen, the attacker gets random bytes.

The design that satisfies this:

Each note has a random 256-bit symmetric key (the "note key"). Note content is encrypted
with this key using AES-256-GCM. The note key is never stored anywhere in plaintext — it
is always wrapped (encrypted) using the owner's public key via X25519 ECDH.

The mechanics of key wrapping: generate a random ephemeral X25519 keypair, compute a
Diffie-Hellman shared secret with the recipient's long-term public key, derive a wrapping
key via HKDF, and encrypt the note key with AES-256-GCM. The result (ephemeral public key
+ ciphertext + nonce) is the "key envelope." The server stores one key envelope per
authorized user per note.

This scheme has one particularly elegant property: sharing a note with a new user does not
require re-encrypting the note content (which may be large). It only requires re-wrapping
the 32-byte note key for the new recipient — an operation that is O(1) in note size.

On Apple platforms, all of this is available through CryptoKit (built into macOS and iOS),
which means no third-party cryptography dependencies at all. The implementations are
memory-safe, audited by Apple, and FIPS-compliant where relevant.

### The echo guard problem

There is a subtle bug that E2E encryption introduces into the existing sync logic.

The current clients have an echo guard: when the server sends an update, the client
stores that content as `lastReceivedContent`. When the debounce fires, it checks whether
the current content equals `lastReceivedContent` — if so, it skips the send, because
sending back what you just received would create an infinite loop.

With encryption, this comparison breaks completely. AES-256-GCM uses a random 12-byte
nonce on every encryption. Two encryptions of the same plaintext produce completely
different ciphertext. So the client would always see "this ciphertext differs from what
I received" and send, even for content that hasn't changed.

The fix: replace the content equality check with a timestamp check. If the current
`updatedAt` equals the `updatedAt` of the last received message, there is nothing new
to send. This is actually more correct than the content check was — timestamps are the
authoritative source of "has this changed," not the content itself.

### What sharing actually means with E2E encryption

This is where the design gets interesting. In a system without encryption, sharing is
just an access control record in the database. But with E2E encryption, the server never
has access to the note key — so it cannot re-encrypt the note for a new collaborator.
Only the sharer can do this, because only the sharer has their own private key.

This means sharing requires the sharer's device to be online at the time of sharing.
There is no way around this in a true E2E system. The server cannot share on the owner's
behalf. This is a deliberate tradeoff: you give up the ability to delegate sharing to
the server in exchange for the server being cryptographically unable to read your content.

Revoking access is similarly constrained. Removing a collaborator from the `note_shares`
table prevents future access. But if the revoked user downloaded and cached the key
envelope, they can still decrypt what they already have locally. Full revocation requires
rotating the note key — re-encrypting the content with a new key, re-wrapping that key
for all remaining collaborators, and uploading everything. This is an expensive but correct
operation.

For the POC, soft revocation (remove from database) is sufficient. Document the limitation.

### Implementation order

The four features have hard dependencies:

1. **User accounts first** — everything else requires knowing who is making a request
2. **Multiple notes** — sharing requires there to be individual notes to share
3. **E2E encryption** — sharing requires the encryption layer to already exist
4. **Sharing** — requires both multiple notes and E2E encryption

Folders are independent once multiple notes exist. They can be built alongside phase 2.

> 📝 *Write here: what was it like thinking through the encryption design? Was it
> intimidating? Did you expect it to be more complicated than it turned out? What does
> it feel like to know that the server you're building genuinely cannot read your data?*

### Where things stand

The architecture is decided. The database schema is designed. The implementation order
is clear. Nothing has been built yet in this session — this was a planning session.

But planning sessions have value. The decision to use a modular monolith, the SQLite
driver choice, the echo guard fix, the insight that sharing requires the sharer to be
online — these are the kinds of things that would have been discovered painfully during
implementation if not reasoned through first.

The next session starts with Phase 2a: restructure the server into its modular layout,
add SQLite, and implement user registration and login with JWT auth.

> 📝 *Write here: do you normally plan this carefully before coding, or do you usually
> figure it out as you go? What made you want to think through the full architecture
> before touching the server?*

---

## March 9, 2026 (night, continued) — The macOS client, rewritten

### From one note to many: what had to change

The old macOS client was five files, ~350 lines total. The new one is six files and about
550 lines. It is not dramatically larger, but almost every line changed.

The old `NoteViewModel` managed one note. The new `NotesViewModel` manages a collection of
folders and notes, which note is selected, which note is in the editor, and how to flush
unsaved changes when the user switches away.

The old `LocalStore` read and wrote a single `note.json`. The new one reads and writes
`data.json` — an object with a `folders` array and a `notes` array, same format as the
server's persistence file.

The old `SyncService` called back with `(content: String, updatedAt: Int64)`. The new one
calls back with a `WSMsg` — a tagged union covering all twelve message types. The view model
switches on the type and handles each one.

### The three-column layout

SwiftUI's `NavigationSplitView` on macOS gives you the three-column layout from Apple Notes
for free. It handles the sidebar toggle, the column dividers, the minimum/maximum widths.
The only work is populating each column:

```
NavigationSplitView {
    // Left: folder list
} content: {
    // Middle: notes in selected folder
} detail: {
    // Right: title field + text editor
}
```

The selection is driven by `List(vm.folders, selection: $vm.selectedFolderID)`. SwiftUI
highlights the selected row and binds the selection to the view model automatically. Same
for the note list. No custom selection tracking needed.

### The flush-on-switch problem

The trickiest part was making sure edits are saved when the user switches notes without
waiting for the 500ms debounce.

The sequence:
1. User is typing in note A (debounce running, 500ms from last keystroke)
2. User clicks note B in the list
3. Note B's content should appear immediately
4. Note A's changes should be saved right now, not 500ms later

The solution: the note list's `.onChange(of: vm.selectedNoteID)` calls
`vm.noteSelectionChanged(from: oldID, to: newID)`. This method flushes the old note
synchronously (calls `flushNote(id: oldID, ...)` which saves locally and sends to server),
then loads the new note into the editor.

The debounce still fires 500ms later. But by then, the editor shows note B's content,
and `editingNoteID` is B. If nothing has been typed, `flushNote` detects no change and
does nothing. No duplicate saves, no state corruption.

### CombineLatest for the debounce

The old debounce observed `$content` (a single `String`). The new one has two fields:
`editingTitle` and `editingContent`. Instead of two separate debounces, one combined one:

```swift
Publishers.CombineLatest($editingTitle, $editingContent)
    .dropFirst()
    .debounce(for: .milliseconds(500), scheduler: RunLoop.main)
    .sink { [weak self] title, content in
        guard let self, let noteID = self.editingNoteID else { return }
        self.flushNote(id: noteID, title: title, content: content)
    }
```

This fires once, 500ms after the last change to either field. A single network message
covers both title and content.

### WSMsg encoding: omitting nil fields

Swift's `JSONEncoder` encodes `nil` optionals as `null` by default. Sending
`"folder_id": null` in an `update_note` message is wrong — the field should not be there
at all. The fix: a custom `encode(to:)` using `encodeIfPresent` for every optional field.
The decode side uses `decodeIfPresent` symmetrically.

### Build: 1.7 seconds

`swift build` completed in 1.7 seconds. No warnings. The old single-note client compiled
in about the same time — a useful check that we haven't added unnecessary complexity.

> 📝 *Write here: what was it like rewriting an app you built two days ago? Did it feel
> like throwing away work, or like the first version doing its job (proving the idea so
> you could build it properly)? Was the rewrite faster or slower than the original build?*

### Where things stand

Server and macOS are in sync on the new protocol. Windows and Linux are still on the old
single-note protocol — they will need updating in a future session.

The next logical steps:
1. Bring Windows and Linux to feature parity (folders + multiple notes UI)
2. Then add auth across all clients at once — rather than adding auth to macOS first and
   having to update Windows and Linux twice

---

## March 9, 2026 (continued) — The Windows client catches up

### What had to change

The macOS rewrite was the template. The Windows client needed to do the same thing:
go from a single `TextBox` to a three-column layout with folders, a note list, and a
note editor. The scope of change is roughly the same.

The single-note Windows app was four files. The new one is six:

- `Models.cs` — `Folder`, `Note`, `FolderItem`, `WsMessage` (replacing the old single-note types)
- `NotesViewModel.cs` — all business logic (replacing the old `NoteViewModel.cs`)
- `LocalStore.cs` — now reads/writes `data.json` with `folders` and `notes` arrays
- `SyncService.cs` — now delivers typed `WsMessage` objects covering all twelve message types
- `MainWindow.xaml` — three-column Grid layout with DataTemplates for the two ListViews
- `MainWindow.xaml.cs` — event wiring, `_suppressEditorChanged` flag, dialog helpers

The old `NoteViewModel.cs` was deleted. `NotesViewModel.cs` is its replacement.

### The ObservableCollection diff

This was the most interesting Windows-specific problem.

WinUI 3's `ListView` works well with `ObservableCollection<T>`. The naive approach —
clear the collection and repopulate it on every folder switch or sync message — is
correct but destructive: clearing the collection clears the selection, which fires
`SelectionChanged`, which calls `NoteSelectionChanged`, which wipes the editor. The user
clicks a folder and their current note vanishes.

The fix: never clear `FilteredNotes`. Instead, maintain it via a diff algorithm:

1. Compute the target list (notes for the selected folder, sorted by `updatedAt` descending)
2. Pass 1: remove items from `FilteredNotes` that are not in the target
3. Pass 2: for each item in the target, either insert it (if new) or `Move()` it to
   the correct position (if already present but out of order)

`ObservableCollection.Move()` fires `NotifyCollectionChangedAction.Move` — the ListView
reorders the row without touching selection. The selected note stays selected through
folder switches, syncs, and new note arrivals. No editor flicker.

The same principle applies to `Note` itself. `Note` implements `INotifyPropertyChanged`.
When a `note_updated` arrives, we find the existing `Note` object and set its properties
in-place. The ListView row refreshes without the item being removed and re-inserted.
Selection is preserved throughout.

### The flush-on-switch problem

Same problem as macOS, different API.

When the user clicks a different note, the editor must save the current note immediately —
not wait 500ms. The `ListView.SelectionChanged` event fires on the UI thread and has
access to the current `TextBox` values. The code-behind passes them to
`NoteSelectionChanged(oldId, newId, title, content)`. The view model cancels the pending
`CancellationTokenSource`, flushes the old note synchronously (save + send), then loads
the new note's content back into the editor.

The `_suppressEditorChanged` flag handles a follow-on issue: when the editor TextBoxes
are updated programmatically (loading a note), that fires `TextChanged`, which would
start a new debounce for content that came from the model and does not need to go back.
The flag is set before the programmatic update and cleared immediately after.

### The callbacks architecture

The macOS view model uses `@Published` properties that SwiftUI observes automatically.
WinUI 3 has data binding too, but the editor's two-way interaction with the flush-on-switch
logic made explicit callbacks cleaner. Three callbacks are injected into the view model
constructor:

- `onEditorChanged(title, content)` — update the editor TextBoxes
- `onConnectionChanged(bool)` — update the status dot colour
- `onNoteAutoSelected(id, title, content)` — select a newly-created note in the ListView

Data flow is explicit: view model calls a callback, code-behind updates the UI. The
`ObservableCollection`s are bound directly in XAML for the folder and note lists — that
part uses standard binding. The editor is handled via callbacks because it needs
synchronous, ordered updates that interact with flush logic.

### The "All Notes" sentinel

Folders and "All Notes" need to coexist in one `ListView`. The solution is a flat
`FolderItem` class used for both. "All Notes" gets a fixed sentinel ID (`"__all__"`).
`FolderItem.IsAllNotes` returns `true` for this sentinel, and the XAML DataTemplate uses
it to conditionally hide the rename/delete buttons. The view model uses the same ID to
decide whether to show all notes or filter to one folder.

### Where things stand

Server, macOS, and Windows are all on the Phase 2b protocol: three-column layout,
folders, multiple notes, offline-first, debounced sync, per-note last-write-wins.

Linux is still on the old single-note protocol and will need the same treatment.
After Linux reaches parity, the next milestone is Phase 2a: user accounts and auth
across all four platforms at once.

> 📝 *Write here: what was the experience of implementing the same feature back to back
> on two different platforms? Did the macOS version make the Windows one easier, or did
> the different APIs feel like starting from scratch? What was the most surprising
> difference between the two implementations?*

---

## 2026-03-09 — Linux catches up: multi-folder/multi-notes with GTK4

### What was built

Rewrote the entire Linux GTK4 client from the old single-note architecture to full
Phase 2b parity with macOS and Windows. The app now has the same three-column layout
(folder sidebar | note list | note editor), the same wire protocol, the same merge
logic, and the same offline-first behaviour.

The key new files:

- **`src/models.h`** (new) — `Folder`, `Note`, `WireMessage` structs. The same
  concepts as `Models.swift` and `Models.cs`, but plain C++ structs.
- **`src/local_store.h/.cpp`** (rewritten) — now reads/writes `data.json` with both
  `folders` and `notes` arrays via json-glib. `note.json` is gone.
- **`src/sync_service.h/.cpp`** (rewritten) — new wire protocol: parses all the
  `folder_created`, `note_updated`, etc. messages; serializes CRUD requests.
- **`src/note_view_model.h/.cpp`** (rewritten) — class renamed to `NotesViewModel`;
  manages `folders_` and `notes_` vectors; handles all message types in
  `handle_message`; `handle_init` does the per-note last-write-wins merge.
- **`src/main_window.h/.cpp`** (rewritten) — three `GtkPaned` columns; `GtkListBox`
  for folders and notes; `GtkStack` switching between "empty" and "editor" pages;
  right-click context menus via `GtkGestureClick` + `GtkPopover`.

### GTK4-specific decisions

**Layout:** Nested `GtkPaned` (horizontal) gives the three-column split. The folder
panel is 200 px and non-resizable; the note list is 260 px and non-resizable; the
editor takes all remaining space. `gtk_paned_set_resize_start_child(FALSE)` locks
the sidebar widths while keeping the editor flexible.

**Folder sidebar header:** `gtk_list_box_set_header_func` adds a "Folders" section
label between the "All Notes" row and the first real folder row — no extra widget
management needed; GTK handles placement automatically.

**Context menus:** GTK4 has no `GtkMenu`. The idiomatic replacement is
`GtkGestureClick` with `button=3` attached to each row (data stored on the gesture
object via `g_object_set_data`), which pops up a `GtkPopover` containing plain
frameless buttons. The popover is unparented in its own `closed` signal to avoid
a leak.

**List rebuild:** The Windows client uses a diff algorithm to preserve `ListView`
selection. GTK's `GtkListBox` has no equivalent of `ObservableCollection.Move()`,
so a full rebuild is used instead: suppress selection signals → remove all rows →
re-add → restore selection by calling `gtk_list_box_select_row`. Clean and correct
at this scale.

**Editor stack:** `GtkStack` switches between an "empty" page ("Select a note to
start editing") and the real editor page (title `GtkEntry` + separator +
`GtkTextView`). The four `suppress_*` flags stop feedback loops when the code
updates widgets programmatically.

**GtkGesture cast:** `gtk_widget_add_controller` takes a `GtkEventController*`.
`GtkGesture` is a subclass but the incomplete-type forward declaration in the header
blocks an implicit cast — `GTK_EVENT_CONTROLLER(gesture)` is required.

**Deprecated API:** `gtk_css_provider_load_from_data` is gone in recent GTK4;
replaced with `gtk_css_provider_load_from_string`.

### What was dropped / not needed

The `NoteData` struct from the original `LocalStore` is gone (replaced by the shared
`models.h` types). The old `note.json` path is no longer written. Users upgrading
from the single-note build will start with an empty local store on first launch —
acceptable for a POC.

### Where things stand

Server, macOS, Windows, and Linux are all on Phase 2b: three-column layout, folders,
multiple notes, offline-first, debounced sync, per-note last-write-wins. The feature
matrix is now uniform across all three desktop platforms.

Next milestone: Phase 2a — user accounts, JWT auth, per-user data isolation.

> 📝 *Write here: how did the GTK4 implementation compare to writing the same feature
> in Swift/SwiftUI and C#/WinUI? What was hardest — the C API wrappers, the context
> menu approach, or keeping the selection state consistent across list rebuilds?*

---

## March 10, 2026 — iOS: the easiest client yet

### What happened

The iOS app is now running on a real iPhone, feature-complete against the other platforms.
All POC features are ✅ across macOS, Windows, Linux, and iOS.

> 📝 *Write here: what made you finally sit down and do the iOS port? Was it the natural
> next step after Linux, or did something trigger it? What was the first moment you saw
> it sync on the phone?*

### How it came together

The iOS client lives in `notes/ios/` and is built with Xcode (`Notes.xcodeproj`).
The structure mirrors the macOS app almost exactly — the same six files, the same
responsibilities, the same sync and storage logic.

`LocalStore.swift` and `SyncService.swift` are effectively unchanged. Swift compiles
for both platforms with zero conditional compilation. The only real difference is the
storage path — iOS uses the app's sandboxed Application Support directory rather than
`~/Library/Application Support/amadeuz/` — which `LocalStore` already handles via
`FileManager.default.urls(for:in:)`.

The view layer needed minor adaptation. The most visible: `NavigationSplitView` on
iPhone collapses to a drill-down stack navigation automatically. SwiftUI handles this
without any extra code — the three-column layout works as-is on iPad and large iPhones
in landscape; on a regular iPhone it becomes folder list → note list → note editor,
which is the correct mobile UX anyway. The framework earned its keep here.

One thing that did not need doing: the `AppDelegate` activation hack from the macOS
app (`NSApp.setActivationPolicy(.regular)` + `activate(ignoringOtherApps:)`). That
was a workaround for an SPM executable quirk on macOS. iOS apps are always foreground
by default. `NoteApp.swift` for iOS is four lines.

### Code sharing observations

The promise of "Swift on Apple platforms shares code" held up well. The logic layer
(`NoteViewModel.swift`) needed only minor adjustments — `UserDefaults` is available
on both platforms, `URLSession` WebSocket is available on both, and the `Combine`-based
debounce is identical. The models are byte-for-byte the same file.

The view layer is where platforms diverge meaningfully, and that is fine. The macOS
`ContentView.swift` uses `NSAlert`-style dialogs implicitly via `confirmationDialog`;
the iOS version uses `.alert` with `TextField` for folder naming inputs. These are
different APIs but the same user intent. Trying to share them would produce worse code
on both platforms.

### Where things stand

All five targets (server, macOS, Windows, Linux, iOS) are now at Phase 2b:
three-column layout, folders, multiple notes, offline-first, debounced sync,
per-note last-write-wins. The feature matrix is uniform across every platform.

Next milestone: Phase 2a — user accounts, JWT auth, per-user data isolation.
The server restructure (SQLite + `internal/` packages) is the blocker; all clients
are ready to add a login screen once the server exposes auth endpoints.

> 📝 *Write here: now that all four clients exist, what does the project feel like?
> Does it feel like a real product yet, or still a POC? What would make it feel real?*

---

## March 10, 2026 — Android: the fifth platform

### Why Android, and why now

After iOS was done, the natural question was: what about Android? There was no strong
reason to wait. The wire protocol is stable. Every client has proven the same architecture
works — view model, sync service, local store. Android is the last major platform
without a client.

The practical question: what is the right stack for a native Android app in 2026?

The answer is not ambiguous. Kotlin has been Google's first-class language since 2017.
Java is legacy — you would only choose it if you had an existing Java codebase to
maintain. Jetpack Compose is the modern Android UI toolkit, declarative and
component-based, with the same mental model as SwiftUI. MVVM with ViewModel and
StateFlow is the Google-recommended architecture and the entire ecosystem is built
around it. There was no real decision to make here.

> 📝 *Write here: do you have prior experience with Android development? Was the stack
> familiar or did you have to learn it from scratch? How does Kotlin feel compared to
> Swift — similar enough to reuse intuitions, or different enough to trip you up?*

### Setting up the environment

The development machine is a MacBook with Homebrew-installed OpenJDK 23. Android Studio
was not installed yet. The setup sequence: download Android Studio, install it, run the
first-launch wizard which downloads the Android SDK (~1-2 GB), verify with `adb` in
`~/Library/Android/sdk/platform-tools/`.

The Java installation from Homebrew is fine — Android Studio actually ships with its own
bundled JDK and manages it independently. The Homebrew JDK is used by the Gradle daemon
but the two coexist without conflict.

### Creating the project

Android Studio's "Empty Activity" wizard generates a working Kotlin + Jetpack Compose
project. The template produces: `MainActivity.kt` with boilerplate, a `ui/theme/`
directory with Material 3 color/typography setup, and `build.gradle.kts` files using the
Gradle version catalog (`libs.versions.toml`) for dependency management. This is the
current standard — no more manually typed version strings scattered across build files.

### The architecture maps cleanly

Every concept from the other clients has a direct Android equivalent:

| macOS (Swift) | Android (Kotlin) |
|---|---|
| `@Published` + `ObservableObject` | `MutableStateFlow` + `StateFlow` |
| `Publishers.CombineLatest` debounce | `viewModelScope.launch { delay(500) }` |
| `URLSessionWebSocketTask` | OkHttp `WebSocket` |
| `DispatchQueue.main.async` | `viewModelScope.launch(Dispatchers.Main)` |
| `UserDefaults` | `SharedPreferences` |
| `NavigationSplitView` (three columns) | `ModalNavigationDrawer` (mobile drawer) |

The one layout difference is deliberate. Three-column layouts work on desktop and tablet,
but on a portrait phone screen a persistent sidebar wastes space. The Android client uses
a navigation drawer — swipe or tap the hamburger icon to see folders, tap a note to open
the editor full-screen. This is the correct mobile UX pattern, and it happens to simplify
the Compose code compared to managing three simultaneous columns.

### Two build issues

The project did not build cleanly on the first try. Two issues:

**1. The kotlin-android plugin conflict.**
Adding `org.jetbrains.kotlin.android` to `plugins {}` — the standard advice for Kotlin
Android projects — caused a crash: "Cannot add extension with name 'kotlin', as there is
an extension already registered with that name." AGP 9.x (the version generated by Android
Studio in 2026) applies the Kotlin Android plugin internally. Adding it again causes a
conflict. The fix: remove it from both `build.gradle.kts` files. The `kotlin.compose`
plugin (for the Compose compiler) must still be added explicitly.

This is the kind of thing that trips you up when documentation lags behind tooling
releases. The standard "how to set up Kotlin Android" instructions say to add the plugin.
That was true in AGP 8.x. In AGP 9.x it is wrong and will break your build.

**2. Missing icons.**
`Icons.Default.Circle` and `Icons.Default.CreateNewFolder` do not exist in
`material-icons-core`. They are in `material-icons-extended` — a separate, much larger
library (~10 MB) that contains hundreds of additional icons. Rather than adding a heavy
dependency for two icons, the workarounds are:

- `Circle` → a `Box(Modifier.size(10.dp).background(color, CircleShape))`. A
  coloured dot made from a `Box` with a circular background shape. No icon needed.
- `CreateNewFolder` → `Icons.Default.Add`. Different icon, same intent.

> 📝 *Write here: how did it feel to hit two build errors before writing a single line of
> app logic? Is this kind of friction expected in Android development, or was it a
> surprise? Did fixing it feel like progress or just overhead?*

### What was not done yet

The app was written on a MacBook that is too slow to run the Android emulator comfortably.
The code is complete and reviewed, but has not been run on a real device or emulator yet.
This will happen on a faster Windows machine where the emulator can run properly.

This is noted honestly because it matters: "code is written" and "it works" are different
things. The feature matrix uses ⏳ for Android precisely to capture this distinction.
The code has been read, the logic verified against the other clients, the build errors
fixed — but no note has been typed and synced from Android yet.

### Where things stand

Five clients now have complete code: macOS, Windows, Linux, iOS (all verified running),
and Android (code complete, pending first device run).

The next milestone is still Phase 2a: user accounts and JWT auth. Once Android is
confirmed working on a real device, the codebase will have full coverage across every
major platform before auth is added.

> 📝 *Write here: what does it mean to you to have clients for every major platform —
> Apple (macOS + iOS), Android, Windows, Linux? Was that the original plan, or did it
> evolve? What is the actual end-user scenario you're imagining when all of these sync
> together?*

---

## March 11, 2026 — Note organisation: folders become optional, move, search

### What we built today

Four features, all on the same theme: making notes easier to organise and find without
imposing structure on the user.

**1. Folders are now optional**

Until today, creating a note required selecting a folder first. There was no way to
just open the app and start writing — you had to pick (or create) a folder, then
create a note inside it.

The fix was smaller than expected. On the server, `CreateNote` previously checked that
the given `folder_id` referred to an existing folder and rejected the creation if not.
Removing that check and letting `folder_id` be an empty string was the entire server
change. The `create_note` message no longer requires `folder_id`.

On the Mac client, the "New Note" toolbar button was disabled when "All Notes" was
selected. Removing that condition and teaching `createNote()` to send no `folder_id`
when in the "All Notes" view was enough. Notes with no folder appear in "All Notes"
and in no specific folder — exactly the right behaviour.

We also removed a piece of bootstrap logic that had been quietly creating a default
"Notes" folder on first server connect. That was there precisely because notes
required a folder. Without the requirement, the bootstrap has no reason to exist.

> 📝 *Write here: did the "folder required" design ever bother you during testing?
> Was there a specific moment — trying to jot something down quickly and having to
> stop and make a folder first — that made this feel like real friction?*

**2. Move note between folders**

Notes can now be moved from one folder to another, or back to "no folder", via
right-click → "Move to Folder" → submenu. The first item is always "No Folder".
Below a divider are all existing folders.

This required a new wire message pair: `move_note` (client → server) and
`note_moved` (server → all clients). The decision to not reuse `update_note` for
this was deliberate. A move changes `folder_id` only, not content. Using
`update_note` would force the client to send the full note content just to change
the folder, and the last-write-wins timestamp guard could theoretically reject
a move if a concurrent content edit came in first. A dedicated message is cleaner
and semantically unambiguous.

The move also bumps `updated_at` so that all other clients receive and apply it
correctly through the existing last-write-wins flow.

**3. Folder label in note rows**

Each note row in the list now has a third line (below the content preview) showing
the folder name with a folder icon. If the note has no folder, nothing is shown —
the row simply has two lines instead of three. No placeholder, no empty space.

The implementation is a `folderName: String?` parameter on `NoteRow`, resolved at
the call site with a single lookup: `vm.folders.first { $0.id == note.folderID }?.name`.
Returns `nil` for empty `folderID` naturally. A simple `if let folderName` conditional
renders the third line only when there is something to show.

**4. Search**

A search field in the top-right toolbar filters the note list as you type. Matching
is a case-insensitive substring check against both title and content.

When you start typing, the app automatically switches to "All Notes" so the search
runs across the entire collection, not just the currently selected folder. The folder
sidebar stays visible — you can see which folder each result belongs to via the new
folder label in the row.

The implementation is entirely client-side. `searchText` as a `@Published` property
on `NotesViewModel` with a `didSet` that flips `selectedFolderID` to `allNotesID`
when non-empty. The `notesInSelectedFolder` computed property applies the search
filter after the folder filter. SwiftUI's `.searchable()` modifier places the native
search field in the toolbar with no manual layout work.

Server-side full-text search is not needed yet — all notes are in memory on the
client and the filter is instant.

> 📝 *Write here: is there a moment when client-side search stops being sufficient?
> When you picture this app with thousands of notes across multiple users, what
> breaks first — the memory, the performance, or something else?*

### What these features have in common

All four features are about reducing friction. Creating a note shouldn't require
picking a folder. Finding a note shouldn't require knowing where you put it.
Moving a note shouldn't require drag and drop.

These are the kinds of details that make the difference between an app you
actually use and one you abandon after a week. They don't make a good demo.
They make a good tool.

> 📝 *Write here: are there other places in the current app where you feel similar
> friction — a step that shouldn't be required, a button that's disabled when it
> shouldn't be, a flow that forces you to think about the app's structure instead
> of what you want to do?*

### Where things stand

The macOS client now has the full feature set for comfortable single-user use:
folders, unfoldered notes, move, search, offline-first sync. The server supports
all of it.

The other clients (Windows, Linux, iOS, Android) are behind on these four features
— they still require a folder to create a note, have no move capability, no folder
label in the list, and no search. These are the next things to port when attention
turns to those platforms.


---

## March 11, 2026 — True offline mode and two client bugs

### The problem with the previous offline story

The app claimed to be "offline-first" — and it was, in the sense that it would load
your notes from disk and let you read and edit them without a server. But there was a
catch: creating a note or a folder required the server. If you were offline and clicked
"New Note", nothing happened. Silently. No error, no feedback, just nothing.

This was a consequence of how creation worked: the client would send a `create_note`
or `create_folder` message to the server, and wait for the server to respond with the
created entity (including the server-generated ID). The client only added the note to
its local state when the server response arrived. No server, no response, no note.

There was also a related bug in `handleInit` — the merge that runs when the client
reconnects after being offline. The comment in the code literally said:

```
// Local-only notes (created offline without server confirmation): dropped.
```

So not only did offline creation not work, any notes that somehow got into local state
without server confirmation were silently deleted on reconnect. The app was lying about
being offline-first for writes.

### The fix: client-generated IDs

The root issue was that the server owned the IDs. To fix offline creation, the client
needed to generate the ID itself, create the entity in local state immediately, and push
it to the server when connectivity was available.

The server change was small: `CreateFolder` and `CreateNote` now accept an optional
client-provided ID. If present, the server uses it. If absent, the server generates one.
This makes the existing online flow (no ID sent) continue to work identically, while
enabling the offline flow (client sends its own UUID).

The client change was larger. `createFolder` and `createNote` now:
1. Generate a UUID
2. Build the entity locally
3. Append it to state and save to disk
4. Optionally push to server if connected

`handleInit` (the reconnect merge) was also fixed. Local-only folders are now preserved
and pushed to the server instead of being overwritten. Local-only notes are now kept
and pushed with their full content (including offline edits) instead of being dropped.

The result: the client works identically whether connected or not. The server is now
truly a sync accelerator, not a dependency for basic use.

> 📝 *Write here: was there a specific time when you were away from your home network,
> opened the app, and ran into this wall? Or was it a "this will embarrass me if
> I show someone" realization when reviewing the code?*

### Two UI bugs discovered while testing

**Bug 1: new note sometimes shows old content**

After fixing offline creation, a pre-existing bug became more obvious. Creating a note
would sometimes show the previously selected note's content in the editor of the new
(empty) note.

The cause was subtle. In SwiftUI, `.onChange(of:)` only fires when the observed value
changes *while the view is already mounted*. The note list in the middle column is a
`List` — but only when there are notes to show. When the list is empty, the view shows
`ContentUnavailableView` instead, and the `List` is not in the hierarchy at all.

When you create the first note in a folder (or the first note with no folder selected):
- `notes.append(note)` and `selectedNoteID = note.id` both fire in the same synchronous
  block, so SwiftUI batches them into a single render cycle
- The `List` appears for the first time with the selection already set
- `onChange` never sees a change — the view was just mounted with that value
- `loadNoteIntoEditor` is never called
- The editor still shows whatever it was showing before

The fix: call `loadNoteIntoEditor` directly in `createNote()`, before setting
`selectedNoteID`. This is unconditional — it does not depend on the `List` being
mounted. The `onChange` path remains for user-driven selection changes and is harmless
if called twice (idempotent).

**Bug 2: note row height doesn't update after moving to a folder**

Each note row shows three lines: title, content preview, and folder name.
When a note has no folder, the folder line was hidden (using `if let folderName`),
making the row two lines tall. When the note was moved to a folder, the folder line
appeared — but SwiftUI had cached the row height at two lines and would crop the
newly visible third line.

The fix was simpler than the diagnosis: always show the third line. When there is no
folder, show "—" instead of hiding the label. The row is always three lines tall, so
SwiftUI never needs to re-measure it.

### Where things stand

The macOS client is now genuinely offline-first for all operations: create, edit,
delete, move. The server handles sync when available; when it is not, everything
still works and nothing is lost.

The other platforms (Windows, Linux, iOS, Android) still have the original
server-dependent creation flow. They will need the same client-generated ID
treatment when it is their turn.

---

## 2026-03-11 — Session 2: inline images + Markdown styling

*(Same day, second session. The previous session closed with offline-first note/folder creation working on macOS. This session tackled a much bigger scope than expected.)*

### The question: how do you put images in a note?

The user asked for images interleaved with text, cross-platform, with no browser engine. Three options were on the table:

1. **Base64-embed images in the Markdown string.** Simplest to implement, but inflates note size dramatically and would destroy WebSocket framing for any image larger than a thumbnail. Rejected.

2. **NSTextView block document model.** Treat the note as a sequence of typed blocks (text, image, table). Rich and extensible, but every platform needs its own block renderer and there is no universal serialisation format. Rejected — too much complexity for a first pass.

3. **Markdown text + separate blob store.** Note content is plain Markdown. Images are referenced as `![](amadeuz://blob/<uuid>)`. The binary data lives in a flat blob store on the server, fetched by UUID. This keeps the note as a plain string — easy to persist, sync, search — while offloading binary handling to a dedicated layer.

Option 3 was the clear winner. The Markdown string is already the sync unit; the blob is immutable content-addressed by UUID. These are separable problems.

### The editor: NSTextView

`TextEditor` (SwiftUI's built-in text field) wraps `NSTextView` but exposes almost none of its power. The only way to get inline images in a native macOS editor is to go straight to `NSTextView` via `NSViewRepresentable`.

The result: `MarkdownEditor.swift`, which wraps `NSTextView` with a custom `NSTextAttachment` subclass called `BlobAttachment`. The attachment stores a `blobID` alongside its image data, so the serialiser can reconstruct the `amadeuz://blob/<uuid>` Markdown reference from the in-memory attributed string.

Two directions of conversion needed:
- **Load** (`buildAttributedString`): parse Markdown → split on image references → create `BlobAttachment` for each image reference, fetch image from blob store → `NSAttributedString`
- **Save** (`extractMarkdown`): enumerate attachments in `NSAttributedString` → reconstruct Markdown string with `![](amadeuz://blob/<uuid>)` references in the right positions

The round-trip is clean because `extractMarkdown` only looks at `.attachment` attributes, ignoring all the visual styling (font, color) that gets layered on top.

> 📝 *Write here: first impressions of NSTextView vs the SwiftUI TextEditor you had before. Was the loss of "it just works" painful? Or did you immediately feel more in control?*

### Dark mode: the first bug

With the new `NSTextView` in place, the text was black — invisible on a dark theme. `TextEditor` handled this automatically. Raw `NSTextView` does not.

The fix touched three places:
1. `defaultAttrs` in `buildAttributedString`: add `.foregroundColor: NSColor.labelColor`
2. `tv.textColor = .labelColor` in the view setup
3. `tv.typingAttributes[.foregroundColor] = .labelColor` in the view setup

`NSColor.labelColor` is a dynamic system color that resolves to white on dark, black on light. It is the right default for body text.

### The typing-after-image bug

After fixing dark mode globally, a subtler variant appeared: text typed immediately after an image went black.

The cause: `NSAttributedString(attachment: att)` — the standard initialiser — produces a one-character attributed string with no `foregroundColor`. When `NSTextView` derives `typingAttributes` for the next character, it samples the character before the cursor. If that character is a color-less attachment, `typingAttributes` comes back with no foreground color, which `NSTextView` renders as black.

Two fixes together closed this:
1. `attString(for:)` helper — wraps any `BlobAttachment` in an `NSAttributedString` that explicitly carries `.foregroundColor: NSColor.labelColor`. Used everywhere an attachment is inserted.
2. `restoreTypingAttributes()` — called after any programmatic insert; forces `typingAttributes[.foregroundColor] = .labelColor`.

The same bug appeared when *loading* a note that already contained an image: `buildAttributedString` was also using the bare `NSAttributedString(attachment:)` initialiser. Fixed in the same pass.

> 📝 *Write here: this is the kind of bug that makes you appreciate platforms that are fully declarative. With NSTextView you are managing state that SwiftUI would normally hide from you. Did it feel like a fair trade?*

### Extra note row height from the image emoji

The note list preview was replacing image references with a `📷` placeholder. This was a mistake: emoji have larger font metrics than `.caption` text, which caused an extra margin between the content preview and the folder label. Replaced the emoji with an empty string. Collapsed consecutive blank lines with a regex. Row height became stable.

### Markdown rich text: a surprise delight

The user asked for simple Markdown formatting — headers, bullets, checkboxes — "since we're already in Markdown". With the `NSTextStorage` infrastructure already in place, this was a natural extension: `applyMarkdownStyling()` adds visual attributes (font, color, strikethrough) after each change without touching the underlying text.

Features implemented:
- `# ` → system bold font +9pt; `#` characters dimmed to `tertiaryLabelColor`
- `## ` → system bold font +5pt; `##` dimmed
- `### ` → system bold font +2pt; `###` dimmed
- `- ` → `-` dimmed to `tertiaryLabelColor`
- `- [ ] ` → `[ ]` in `secondaryLabelColor`
- `- [x] ` → `[x]` in `systemGreen`, rest of line in `secondaryLabelColor` with strikethrough

The user asked why there is no third-party library for this — "if it were a web app, we'd just install a dependency". The honest answer: the ecosystem is thinner and the approach is different. Native macOS rich text typically uses `NSAttributedString` directly, which is lower level but also more flexible. There are some libraries, but they are usually opinionated about the full editing experience and tend to conflict with anything custom. For a narrow feature like Markdown syntax highlighting over an existing `NSTextView`, rolling it yourself is usually less work than integrating a library that was designed for different assumptions.

### The scroll jump

With Markdown styling working, a new symptom appeared: pressing the spacebar caused the scroll position to jump down and snap back. Not a rounding error — it was a full-document layout invalidation on every keystroke.

The root cause: `applyMarkdownStyling()` was calling `storage.addAttribute(range: full document range)` on every character change. This forced `NSLayoutManager` to recompute the entire document's layout, which changed the document's estimated height, which shifted the scroll offset.

The fix: a dedicated fast path `applyMarkdownStylingForCurrentLine()` that computes `paragraphRange(for: cursorPosition)` and operates only on that range. Full-document styling is still called on note load and paste. Per-keystroke styling only touches the paragraph being edited. Scroll position is stable.

### The offline image problem

After testing with the server down, it was obvious: the original image implementation was not offline-first at all.

The flow was:
1. User pastes an image
2. Client uploads to server, gets back a blob ID
3. Client inserts the image with the server-assigned ID

Step 2 fails offline. The image is never inserted.

This was the same conceptual mistake that was made earlier with note/folder creation — letting the server own the identity. The fix follows the same pattern:

**Client generates the UUID.** The image is saved to local disk immediately. It is inserted into the note with the final ID before any network call. Then a background task attempts the upload via `PUT /blobs/:id`. If the upload fails, the blob ID is added to `pending_blobs.json`. On the next reconnect, `uploadPending()` retries all queued blobs.

The server's `PUT /blobs/:id` endpoint is idempotent: if the blob already exists, it returns 200 without re-writing. Retries are safe.

The `BlobStore` class manages:
- Local cache (`~/Library/Application Support/amadeuz/blobs/`)
- Pending upload queue (`pending_blobs.json`)
- `save(_ data: Data) throws -> String` — synchronous local save, returns UUID
- `upload(id: String) async throws` — `PUT /blobs/:id`, removes from pending on success
- `uploadPending() async` — called by `NoteViewModel` on reconnect

> 📝 *Write here: the pattern of "client owns the identity, server is just a mirror" is emerging as a strong principle across this project. Does it feel natural now? Did the first time you got it right (notes/folders) make the blobs version obvious?*

### The note creation overwrite bug

While testing offline image insert, a more serious bug surfaced: creating a new note was sometimes overwriting existing notes with empty content.

**Scenario 1 (offline → online):** Go offline, create a note, type in it. Come back online. Hit "new note". The previously created note's content is wiped.

**Scenario 2:** Select a note, create a new note from the toolbar. The old note is overwritten with empty content.

The cause was a race condition in the SwiftUI `onChange` + `NoteViewModel` interaction:

1. `createNote()` calls `loadNoteIntoEditor(newNote)` — this sets `editingNoteID = newNote.id`, `editingTitle = ""`, `editingContent = ""`
2. `createNote()` then sets `selectedNoteID = newNote.id`
3. SwiftUI's `onChange(of: selectedNoteID)` fires, calling `noteSelectionChanged(from: oldNoteID, to: newNoteID)`
4. `noteSelectionChanged` sees `oldNoteID` is non-nil and calls `flushNote(oldNoteID, editingTitle, editingContent)`
5. But `editingTitle` and `editingContent` are now `""` — step 1 already cleared them
6. `flushNote` writes `""` and `""` to the old note — content destroyed

The fix: guard the flush with `old == editingNoteID`. By the time `onChange` fires, `editingNoteID` has already been updated to `newNote.id` by `loadNoteIntoEditor`. The condition `old == editingNoteID` is false, so the flush is skipped. The old note's content is never clobbered.

This guard is a clean invariant: "flush the editor into note X only if the editor is currently showing note X." The `createNote()` path violates that invariant (the editor was handed to the new note before `onChange` fires), and the guard correctly detects the violation.

### Where things stand

The macOS client now has:
- Inline images: paste or drag any image into a note; images sync cross-device; work offline
- Markdown styling: headers at three levels, bullet lists, and checkboxes with live rendering
- Scroll-stable per-keystroke styling
- Offline-first blob store with automatic retry on reconnect
- No note data loss on create

---

## March 11, 2026 — Planning Phase 2a: auth strategy and the password recovery decision

### The plan going forward

Decided to focus Phase 2a on server + macOS only, validate the architecture end-to-end, then bring parity to Windows, Linux, iOS, and Android. Building auth across five platforms simultaneously before the design is proven would multiply the cost of every wrong decision.

### Password recovery without email

The first real product decision of Phase 2a: how do users recover a forgotten password without involving email?

Email is off the table. It requires SMTP infrastructure (or a third-party email service), which is a self-hosting burden and an external dependency — exactly the kind of thing the target user is trying to avoid. The audience for Amadeuz is people who run their own infrastructure precisely because they don't want to trust third parties. Adding a dependency on Mailgun or asking them to configure Postfix defeats the purpose.

Researched how other self-hosted projects handle this:

- **Nextcloud** and **Gitea/Forgejo**: CLI reset that talks directly to the database. No email required. This is the documented recommended path, not a workaround.
- **Miniflux**: email reset was never built at all. CLI reset is the only method. Cleanest implementation reviewed.
- **Vaultwarden** and **Standard Notes**: no viable recovery path. The master password is the encryption key derivation input — resetting it destroys the vault. Their honest answer is "keep an export."

The Vaultwarden/Standard Notes problem is only relevant when you tie encryption keys to passwords (Phase 2c). For Phase 2a (plaintext SQLite, auth only), the right model is Gitea/Miniflux: CLI reset + recovery codes.

**Decision:** Recovery code at registration. The server generates a high-entropy random string on signup. The user saves it. Presenting the code lets them reset the password. Admin CLI reset (`amadeuz-server reset-password --user ...`) as a last resort.

> 📝 The research surfaced something interesting: Miniflux deliberately never built email reset. It's a choice, not an oversight. There's something appealing about a tool that's honest about what it is and who it's for.

### Why E2E encryption matters more than it seems

Also clarified the threat model for E2E encryption. On a LAN (Raspberry Pi at home), E2E is a nice-to-have — you own the physical network. But the more common deployment pattern is a VPS somewhere on the internet so you can access your notes from your phone on the go.

On a VPS, the hosting provider has root access to the machine and the SQLite file. Without E2E, moving off iCloud to avoid Apple reading your notes just trades Apple for Hetzner (or AWS, or DigitalOcean). The server stores plaintext. Legal orders to the hosting company produce everything.

E2E encryption is what makes the VPS deployment honest about its privacy promise. The hosting company stores ciphertext they can't read. Keys never leave the user's devices. This is why the privacy-conscious audience will trust the product with sensitive content — not just shopping lists, but medical notes, private journals, credentials.

E2E is not deferred because it's a nice feature. It's deferred (to Phase 2c) because the auth layer needs to exist first — you can't wrap keys per user before users exist. The sequencing is forced, not a choice to skip it.

### Where things stand

Phase 2a is ready to start:
- Server: SQLite migration, modular monolith restructure, JWT auth, register/login endpoints
- macOS: login/register UI, JWT in Keychain, updated wire protocol
- Recovery: codes at registration, CLI reset as last resort
- No email, ever

The other platforms (Windows, Linux, iOS, Android) still use a plain `TextEditor`/`GtkTextView`/`UITextView` with no image or Markdown support. Images and Markdown are currently macOS-only.

---

## March 11, 2026 — Phase 2a: user accounts, the server rewrite, and the macOS client

### The server rewrite

The Phase 2a server is a complete replacement of the Phase 2b prototype. The old server was a single file with an in-memory store and one WebSocket endpoint. The new server is a modular Go monolith: `internal/db`, `internal/auth`, `internal/notes`, `internal/folders`, `internal/blobs`. SQLite replaces the JSON file. JWT replaces "everyone shares the same notes."

The restructuring was significant but not complicated. Go's `internal/` package boundary does the right thing — the packages can't be imported from outside the module, which keeps the boundary honest. `main.go` is now thin: open DB, wire handlers, start server, wait for signal, drain.

A few decisions made during the server build:

**SQLite via `modernc.org/sqlite`**, not the standard `mattn/go-sqlite3`. The reason is pure Go — no CGO means the binary cross-compiles cleanly to ARM64 for Raspberry Pi without needing a C toolchain on the build machine. One `GOOS=linux GOARCH=arm64 go build` and it's done.

One gotcha: `modernc.org/sqlite` does not honor DSN query parameters reliably. Setting `_foreign_keys=on` in the connection string silently does nothing. `PRAGMA foreign_keys = ON` must be executed as a separate `Exec` call after opening. This took a failing test (`TestFolderDeleteCascadesNotes`) to surface. Similarly, two PRAGMAs cannot be combined in a single multi-statement `Exec` — only the first one runs. Split into separate calls.

**JWT via query param for WebSocket auth.** The WebSocket upgrade is just an HTTP GET. Many client environments (including `URLSessionWebSocketTask` on Apple platforms) do not support custom headers on upgrade requests. The `Authorization: Bearer` header is the right pattern for REST. For WebSocket, `?token=<jwt>` in the URL is the pragmatic choice — universally supported, JWT is already signed so there is no risk of tampering.

**No email for password recovery.** This decision was researched rather than assumed. Looked at how Nextcloud, Gitea, Miniflux, Vaultwarden, and Standard Notes handle this. The pattern for self-hosted tools with privacy-conscious users converges on CLI reset + recovery codes. Miniflux never built email reset at all — it is a deliberate choice, not an oversight. The recovery code model fits: one high-entropy code generated at registration, shown to the user once, stored somewhere safe (password manager, printed paper). Using the code consumes it and issues a new one. No external service, no DNS records, no SMTP configuration, no third-party dependency.

**Recovery codes are single-use.** This matters: a recovery code that can be reused is a second password that never expires. Rotating the code on each use limits the exposure window. If the code is compromised and used by an attacker, the legitimate user's next recovery attempt fails — which is a signal that something went wrong.

### 25 tests before touching the client

Before starting the macOS rewrite, a full integration test suite was written — 25 tests covering every endpoint and every failure path. Auth, folder CRUD, note CRUD, blob upload/download, last-write-wins, user isolation, cascade delete, recovery code rotation, stale timestamp rejection, CLI password reset. All green.

The process of writing tests surfaced two real bugs: the PRAGMA ordering issue described above, and a missing import in `notes/handler.go` that caused a build error when the ping goroutine was first added. Both would have been painful to debug in a running server.

> 📝 *This is the first time in this project that the test suite ran cleanly before a single line of client code was written. How did that change the confidence level going into the macOS rewrite?*

### The macOS rewrite

The macOS client was a complete rewrite. The old client used a single WebSocket for everything — all CRUD operations went through a typed message bus. The new architecture is:

- REST for all mutations (create/rename/delete folder, create/update/move/delete note)
- One WebSocket per open note, for live keystroke sync only
- `@MainActor` on `NotesViewModel` — all state mutation on the main actor, async network calls suspend and release the actor during I/O

The protocol split (REST + per-note WS) is cleaner than the single-bus design. REST is stateless, works with standard HTTP tooling, and maps naturally to the offline queue pattern — if a mutation fails, retry it later. The WebSocket is reserved for the one use case where request/response latency actually matters: seeing someone else's keystrokes appear in real time.

New files: `KeychainStore.swift` (macOS Keychain wrapper for JWT), `APIClient.swift` (typed REST client), `AuthView.swift` (login/register/recover UI). The old `SyncService.swift` became `NoteSync` — a receive-only per-note WebSocket that auto-reconnects and delivers `init` and `update` messages to the view model.

### Three bugs caught during testing

**Recovery code never shown.** The `AuthView` registered a new user, `applyToken()` set `isAuthenticated = true`, which immediately removed `AuthView` from the hierarchy. The sheet for showing the recovery code was attached to `AuthView`, so it never had a chance to appear. Fix: set `pendingRecoveryCode` on the view model *before* calling `applyToken`. Move the sheet to `ContentView`'s root — it's always in the hierarchy regardless of auth state.

**Previous user's data visible after account recovery.** After recovering a password (and therefore logging in), the main screen showed notes from whoever was logged in before. The cause: `fullSync()` merges server state with *local* state. If the local state hasn't been cleared, the previous user's notes are treated as "locally-created" and pushed to the new account. Fix: `applyToken()` clears `folders`, `notes`, the editor, and the local store file before running `fullSync`. This path (active authentication) is distinct from the init path (resuming a session), so offline notes are unaffected.

**400 error on recover.** The server's `/auth/recover` endpoint requires three fields: `email`, `recovery_code`, and `new_password`. The client was only sending two. The `AuthView` also had no "New Password" field in recover mode. The fix required updating `APIClient`, `NoteViewModel`, and `AuthView` together — a reminder that API contracts need to be read carefully before building the client.

### The error message improvement

One last detail: registering with an email that already exists showed "Update conflict" in the UI. The server actually returns the message "email already registered". The `APIError.conflict` case was carrying a hardcoded string.

Fixed by passing the server's plain-text response body through to the error. This is a general mechanism — any future 409 from the server will show the right message without needing client-side string logic.

> 📝 *There's something to write about here: the value of reading error responses instead of mapping status codes to strings. The server already has the right words. The client just needs to not throw them away.*

### Where things stand

Phase 2a is complete on server and macOS:
- New user can register, receive a recovery code, create notes and folders, log out, log back in
- Recovery flow works end-to-end: use the code, get a new password, new recovery code issued
- Notes sync across clients via REST + per-note WebSocket
- Offline-first: create notes without a server, push on reconnect
- All 25 server integration tests passing

The other platforms (Windows, Linux, iOS, Android) still use the Phase 2b architecture — single WS endpoint, no auth. Phase 2a parity for those platforms is the next major milestone before Phase 2c (E2E encryption) can begin.

---

## March 11, 2026 — Linux Phase 2a polish: inline images, auto-login, and list order

*(Same day, continued from the previous session which brought the Linux client to Phase 2a parity.)*

### Three small sessions, three concrete improvements

After Phase 2a landed on Linux — auth, REST CRUD, per-note WebSocket, search, move, Markdown — a few rough edges remained. This session addressed them.

### Inline images: drag and drop

The user tried dragging an image onto the note editor. The default GTK4 behaviour kicked in: the text view accepted the drop as a URI list and inserted the local file path as plain text. Not what anyone wants.

The fix required three pieces working together:

**1. Intercept the drop.** `GtkDropTarget` with `GDK_TYPE_FILE_LIST` attached to the `GtkTextView`. When the signal fires, check the MIME type via `g_file_query_info` — only accept `image/*`. Generate a UUID, copy the file to `~/.local/share/amadeuz/blobs/<uuid>`, then insert.

**2. Display the image inline.** GTK4's mechanism for embedding arbitrary widgets in a text view is `GtkTextChildAnchor` — a placeholder in the `GtkTextBuffer` that renders a child widget at that position. The first instinct was `GtkImage`. The image came out microscopically small. The reason: `GtkImage` is designed for icons; it renders an icon paintable at its natural icon size, not at the pixel dimensions of an arbitrary image. Switching to `GtkPicture` (which is explicitly designed for displaying images, not icons) and adding `gtk_widget_set_size_request(img, disp_w, disp_h)` fixed it. `GtkPicture` renders the scaled pixbuf at exactly the requested dimensions.

**3. Serialize back to Markdown.** The `content_` string stored in the database must be plain text — not binary data, not object replacement characters. A `blob_anchors_` map on `MainWindow` tracks each anchor → UUID. `cb_content_changed` iterates through the buffer character by character using `gtk_text_iter_get_child_anchor()`; when it hits an anchor, it emits `![](amadeuz://blob/<uuid>)` into the output string instead of the U+FFFC replacement character that `gtk_text_buffer_get_text` would normally produce.

The reverse direction — loading a note that contains image references — uses `render_blob_images()`. It scans the buffer for `![](amadeuz://blob/...)` patterns using `gtk_text_iter_forward_search`, collects all matches, then processes them in **reverse offset order** so that earlier deletions don't invalidate later offsets. Each match is deleted and replaced with an inline image anchor.

The whole round-trip — drop → save to disk → insert anchor → serialize as Markdown → reload → render inline — works correctly. Blobs are currently local-only; server upload (`PUT /blobs/:id`) will follow in a future session.

> 📝 *Write here: this is the third time the same pattern has appeared in this project — client generates UUID, stores locally immediately, queues server upload for later. It started with notes and folders, repeated with blobs on macOS, and now again on Linux. Does the pattern feel obvious now, or is it still surprising each time how much simpler it makes the offline story?*

### Auto-login was broken

The user noticed they had to log in every time they opened the app, even though the macOS client remembered the session. The JWT was being stored in GNOME Keyring correctly. The bug was elsewhere.

The `NotesViewModel` constructor is the first thing called in `MainWindow::MainWindow`. If a token is found in the keyring, the constructor calls `on_auth_state_(true, {})` synchronously — which triggers `show_main_page()`. But at that point in the constructor, `window_`, `root_stack_`, and every other widget pointer is still `nullptr`. The `gtk_stack_set_visible_child_name` call is a GTK assertion on a null pointer — a silent no-op.

After the constructor returns, the widgets are built. `GtkStack` defaults to showing the first page added, which is "auth". The existing post-build check was one-sided:

```cpp
if (!vm_->is_logged_in()) {
    // show auth page
}
```

There was no `else` branch. The stack stayed on "auth" even when a valid token was loaded from the keyring.

The fix is two lines: add `else { show_main_page(); }`. The lesson is broader: never rely on side effects from a constructor that fires before the UI exists. Always reconcile UI state explicitly after the widgets are built.

### Note list row order

The note rows were showing: title → folder name → content preview. The correct order, matching macOS, is: title → content preview → folder name. The folder label goes at the bottom because it is the least important piece of information in the row — you already know what folder you're looking at.

This was a two-block swap in `make_note_row()` in `main_window.cpp`. The row height stays constant because the folder label always occupies a line (with opacity 0 and a space placeholder when there is no folder), regardless of order.

### Where things stand

The Linux client now has feature parity with macOS on every Phase 2a feature:

- Auth (login / register / recover / sign out / auto-login via GNOME Keyring)
- REST CRUD for folders and notes
- Per-note WebSocket live sync
- Offline-first (local state loads immediately, server is a background sync)
- Search, move note, unfoldered notes
- Markdown styling
- Inline images (local blobs; server upload pending)

> 📝 *Write here: the Linux client went from zero to feature parity with macOS in a single focused session (plus a polish session). How does that compare to the Windows client, which took longer? Is GTK4 easier to work with than you expected, or harder? What would you tell someone who is thinking about writing a GTK4 app in C++?*

---

## March 13, 2026 — Linux restart: choosing the GNOME-native stack

### The decision

The C++ GTK4 Linux client was complete — Phase 2a feature parity with macOS, including auth, REST CRUD, per-note WebSocket, search, move note, Markdown styling, and inline images. A working, tested codebase. And we decided to throw it away and start over.

Not because it was broken. Because it was built to prove the feature set, and it did that job. The C++ code was always a proof-of-concept written in the fastest language available that would talk to GTK4 — not the language that makes a long-lived GTK4 app pleasant to maintain. With the features validated, the right time to make a stack decision is before the code grows further, not after.

The decision came after a dedicated research pass: Gemini scanned several GNOME Circle projects (iotas, Fragments, Citations, Wordbook, NewsFlash) and wrote three recommendation documents — an architecture guide, a polish guide, and a refactor plan. The external projects are in `external-projects/` for reference.

### What the research confirmed

The GNOME ecosystem has converged on a clear stack for new apps:

- **Rust** for the language. Memory safety without a GC. First-class `async/await` for WebSocket and REST. gtk4-rs bindings are mature and well-maintained.
- **Libadwaita** on top of GTK4. Not optional if you want the app to look native on GNOME 42+. `AdwNavigationSplitView`, `AdwToast`, `AdwStatusPage` — these are the standard building blocks.
- **Blueprint** for UI definitions. The old approach (writing GTK XML by hand, or building the UI in code) is obsolete. Blueprint is declarative, human-readable, and compiles to standard `.ui` XML at build time.
- **Meson + Cargo** for the build. Meson handles Blueprint compilation, GResource bundling, GSettings schema installation, and Cargo integration.
- **GSettings** for config. Replaces the custom `settings.json` from the C++ client.

### One important correction to the research

The architecture guide cited **iotas** as a Rust reference. It is not. iotas is pure Python + PyGObject. The patterns look similar at a high level but translate differently into Rust. **Fragments** is the correct Rust reference — it is a mature, production-quality Rust + GTK4 + Libadwaita + Meson application. Its source is in `external-projects/Fragments-main/`.

### What we deferred deliberately

The research recommendations included several things that are correct for a mature GNOME Circle app but premature for a POC:

- **i18n / gettext** — add later, nothing needs to change structurally
- **Flatpak manifest** — same; add when packaging matters
- **GNOME Shell Search Provider** — definitely post-POC
- **SQLite** for local storage — the flat `data.json` format matches all other clients and is sufficient at this scale

GSettings and GResource were *not* deferred — retrofitting those is painful, and starting without them would mean undoing structural decisions later.

### Where things stand

The legacy C++ code is in `legacy-code/linux/` and is not going anywhere. It is the functional specification for the Rust rewrite: every feature it implements, every piece of business logic, every edge case in the sync algorithm — all of it is available to read. The goal of the Rust client is Phase 2a parity: same features, better code foundation.

The next session starts with scaffolding the Meson + Cargo skeleton.

> 📝 *Write here: how does it feel to delete a working codebase? Is there a moment of hesitation, or does it feel obviously correct? The C++ client took real effort — auth, REST, WebSocket, inline images — and it worked. What is the reasoning you'd give a developer who argues "if it works, don't change it"?*

---

## March 13, 2026 — Phase 1: Rust skeleton scaffolded

The Meson + Cargo + Blueprint skeleton is now in `notes/linux/`. This is the first session on the Rust rewrite.

### What was built

The full directory structure is live:

- **`meson.build`** (top-level): declares dependencies, app ID, profile option, delegates to `data/` and `src/` subdirs.
- **`meson_options.txt`**: single `profile` option (`default` = release, `development` = debug).
- **`Cargo.toml`**: gtk4-rs 0.10, libadwaita 0.8 (v1_7 features), tokio, reqwest, tokio-tungstenite, secret-service, serde — the full dependency set the app will need, so we're not retrofitting later.
- **`data/meson.build`**: calls `gnome.compile_blueprints()` on `window.blp`, bundles the output into a GResource, installs the GSettings schema.
- **`data/ui/window.blp`**: Blueprint file for the main window — `AdwNavigationSplitView` with a sidebar (`ListBox` for notes) and an editor pane (`TextView` with a title `Entry` in the header bar). This is already the full intended layout, not a placeholder.
- **`data/com.amadeuz.Notes.gschema.xml`**: one key — `server-url` — that's all settings this app needs.
- **`src/config.rs`**: the `config_var!` macro pattern from Fragments — reads `MESON_APP_ID`, `MESON_DATADIR`, etc. at compile time. This enforces "build through Meson" rather than plain `cargo build`.
- **`src/main.rs`**: loads the GResource bundle from the installed data path, prints a helpful error with a hint if it's missing, then runs the application.
- **`src/app.rs`**: `AmzApplication` as an `AdwApplication` subclass. Creates `AmzWindow` on activate, reuses it on subsequent activations.
- **`src/ui/window.rs`**: `AmzWindow` as an `AdwApplicationWindow` subclass using `CompositeTemplate`. Binds `split_view`, `note_list`, `new_note_button`, `title_entry`, `text_view` as template children. Wires the two menu actions (`win.show-about`, `win.show-preferences`). The About dialog is a real `AdwAboutDialog` with version pulled from `config::VERSION`.
- **`src/model/`**: `AmzNote` and `AmzFolder` as GObjects with `glib::Properties`. Properties are kebab-case (`folder-id`, `updated-at`, etc.) to match GObject convention. Using `glib::Object::builder()` for construction.
- **`src/backend/`**: three stub modules — `local_store` (load/save `data.json`, uses `glib::user_data_dir()` to locate `~/.local/share/amadeuz/`), `keyring` (async JWT store/load/delete via secret-service, all returning `Ok(None)` for now), `sync_worker` (empty struct holding server URL + token).

### Decisions made

**Blueprint over XML.** The UI is authored in `.blp` and compiled by `blueprint-compiler` at Meson configure time. The compiled `.ui` XML goes into the build directory and is bundled into the GResource. This keeps the UI source readable and forces correct widget naming from the start.

**GResource from day one.** Loading the template from a GResource path (`/com/amadeuz/Notes/ui/window.ui`) rather than a filesystem path means the dev workflow is `meson install -C build --prefix=$HOME/.local` then run the installed binary. Slightly more friction than `./build/binary`, but it mirrors how the app will actually run, and avoids a class of "works in dev, breaks when installed" bugs.

**GObject models upfront.** `AmzNote` and `AmzFolder` are real GObjects with `#[derive(Properties)]`, not plain structs. The extra boilerplate now pays off later when connecting them to `GtkListView` with factory bindings.

**Stubs are real stubs, not deleted code.** `local_store`, `keyring`, and `sync_worker` exist as modules with real function signatures and `// TODO` bodies. The app compiles against them. When Phase 2 fills them in, the interfaces won't change — just the implementations.

### The build workflow

```bash
sudo dnf install meson cargo rust libadwaita-devel libsecret-devel blueprint-compiler
cd notes/linux
meson setup build --prefix=$HOME/.local -Dprofile=development
meson compile -C build
meson install -C build
~/.local/bin/amadeuz-notes
```

The first `cargo build` inside Meson will download crates — takes a few minutes on the first run. Subsequent builds are fast (incremental Cargo).

### Where things stand

The skeleton should compile and open an `AdwApplicationWindow` with the split-view layout visible. No data is loaded, no network calls are made, the note list is empty. The next step is Phase 2: wiring `local_store` so the app loads from `data.json` on startup, wiring the auth flow (login/register/recover screens), and then REST sync.

> 📝 *Write here: first impressions of writing GTK in Rust vs C++. The GObject subclassing boilerplate is significant — every widget is a mod-in-a-mod pattern. Is that more or less annoying than C++ virtual dispatch? What does it feel like to have the compiler catch the things that C++ GTK code left as runtime crashes?*

---

## March 13, 2026 — Phase 2: the full Linux app compiles and runs

### What was built

Phase 1 left us with a skeleton: Meson wired to Cargo, Blueprint compiled, a window opening. Phase 2 filled in everything: auth views, local storage, REST API client, WebSocket sync, full 3-column layout, keyring integration, and the NotesManager event loop that ties it all together.

The compile loop today was a long sequence of errors, each one teaching something about the gtk4-rs ecosystem in 2026.

### The `glib::Sender` removal

The first major surprise: `glib::MainContext::channel::<T>()` no longer exists in glib 0.21. The `glib::Sender` and `glib::Receiver` types that the C++ and older Rust GTK codebases relied on for tokio→GTK communication were silently removed. The replacement is `async_channel::bounded()` paired with `glib::MainContext::default().spawn_local(async move { while let Ok(event) = rx.recv().await { ... } })`. The logic is identical; the API is different.

This is the kind of breaking change that only becomes visible when you actually try to compile against the current versions rather than older examples online.

### Trait bounds on `glib::wrapper!`

Every GObject subclass needs its `@implements` list to match what the parent type actually implements. For a plain `adw::Bin` subclass, you need `gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget`. For an `adw::ApplicationWindow` subclass, you need those three plus `gtk::Native, gtk::Root, gtk::ShortcutManager`. Missing any of these causes a flood of confusing trait bound errors that trace back to `WidgetImpl` and `WindowImpl` bounds. The fix is mechanical once you know the pattern — but tracking it down the first time takes a while.

### GtkStack page names in Blueprint

Blueprint 0.18 has a gotcha with `Gtk.Stack` children: if you write `Gtk.Widget { name: "auth"; }` inside a Stack, Blueprint sets `GtkWidget.name` (the CSS name), not the `GtkStackPage.name`. The page name is what `gtk_stack_set_visible_child_name()` looks up, so the two are completely different things.

The fix is to use explicit `Gtk.StackPage { name: "auth"; child: SomeWidget {}; }` wrappers. Once you know this, it's one extra wrapper per page. Before you know it, you're staring at a "Child name 'auth' not found in GtkStack" runtime warning and the auth screen never appears.

### `gio::Settings::with_path` and the `Result` that vanished

`gio::Settings::new_with_path()` returned a `Result<Settings>` in older gio-rs. In gio 0.21 it was renamed to `with_path()` and now panics if the schema isn't installed. The idiom became: check `gio::SettingsSchemaSource::default().and_then(|src| src.lookup(...)).is_some()` before calling `with_path()`. The net behavior is the same — fail gracefully if the schema hasn't been installed yet — but the API changed under us.

### WebSocket callbacks need `Sync`

`NoteSync::connect` takes closures that are called from inside `tokio::spawn`. That requires `Send`. But the closures are also referenced via `&impl Fn(...)` inside an async block, which requires `Sync` too (you can't share `&T` across threads unless `T: Sync`). The fix is adding `+ Sync` to the bounds. Rust's error message for this is actually good — it suggests the `Sync` addition directly.

### The async send pattern

`async_channel::Sender` is an async sender: `.send(event).await.ok()` in async blocks, `.try_send(event).ok()` in sync closures. The original code used the sync form everywhere and the compiler rejected it in async blocks. Systematic: all `crate::spawn(async move { tx.send(...) })` blocks needed `.await`, and all sync closures passed to `NoteSync::connect` needed `try_send`.

### Where things stand

The app compiles clean (0 errors, 3 harmless dead-code warnings). `ninja -C build && meson install -C build` succeeds. The binary starts, loads the GResource, reads GSettings, checks the keyring for a saved JWT, and presents either the auth screen or the main 3-column layout depending on whether a token is found.

> 📝 *Write here: what it felt like to hit 80 compile errors and watch them reduce to 32, then 6, then 1, then 0. The Rust compiler as a guide rather than an obstacle. Any particular error message that was genuinely helpful? Any that were misleading?*

---

## March 13, 2026 — Markdown formatting and inline images on Linux

### Starting point: looking at Iotas

Before writing anything, we looked at how [Iotas](https://gitlab.gnome.org/World/iotas) — a mature Python + GTK4 notes app — handles markdown. Their approach is two views: a `GtkSource.View` editing widget (raw markdown with syntax highlighting) and a `WebKit.WebView` for the rendered HTML. Smart list continuation lives in `list_formatter.py`, which intercepts the Enter key, checks the current line for bullet/ordered markers, and inserts the appropriate prefix.

We can't use WebKit — the project rules say no browser engine wrappers. So the approach we chose is **live TextTag rendering**: the raw markdown stays in the buffer, and we apply `gtk::TextTag`s to make it look formatted. Headings render larger and bold. `**bold**` appears bold. `*italic*` appears italic. Code spans get a monospace font. This is sometimes called "WYSIWYG live preview" mode — you see the markdown syntax, but it's also visually styled.

### The architecture

Everything lives in `src/ui/md_formatter.rs`. Three concerns:

**1. Tag formatting.** On every buffer change, `apply_formatting()` runs. It gets the text (excluding any child-anchor characters, so image widgets don't throw off byte positions), removes all our `md-*` tags, then re-applies them by feeding the content through `pulldown-cmark`'s `OffsetIter`. The iter gives us `(Event, ByteRange)` pairs. We push a `(tag_name, start_byte)` onto a stack on `Start` events, and pop + apply on `End` events. The tag spans the full element including delimiters (e.g., the `**` are bold, not just the text between them).

The first line always gets the `md-title` tag regardless — because the app treats line 1 as the note title (it's what gets sent to the API's `title` field). Markdown H1-H6 formatting only applies to lines 2+.

**2. Smart list continuation.** `handle_enter_key()` fires before GTK inserts the newline. It reads the current line up to the cursor and checks for bullet markers (longest first to prevent `- ` matching before `- [ ] `). If the line has content after the marker, it inserts `\n<indent><marker>`. If the line contains only the marker (the "I'm done with this list" signal), it deletes the marker and inserts a plain newline. Same logic for ordered lists (`1. `, `1) ` etc.), including auto-incrementing the number.

**3. Inline images.** `embed_images()` runs via `glib::idle_add_local_once` — deferred one GLib iteration so it doesn't run inside the buffer-changed signal. It first removes existing child anchors (they're just 1-char deletions from the buffer), gets the clean text, finds `![alt](path)` patterns with a byte-scanner, and for each local file that exists, calls `buffer.create_child_anchor()` at the end of the `![...]()` span and attaches a `gtk::Picture` widget at that anchor. Images are inserted end-to-start to avoid byte-offset drift.

The `is_formatting` flag prevents `create_child_anchor` (which fires `connect_changed`) from re-entering the formatting loop. The `embed_pending` flag collapses rapid keystrokes into a single idle callback so images aren't re-embedded dozens of times per second.

### Why `pulldown-cmark` and not GtkSourceView?

`gtksourceview5-devel` wasn't installed on the dev machine, and adding a new system dependency for what's essentially a syntax coloring baseline felt wrong. `pulldown-cmark` is a pure-Rust crate, no system deps, and it gives us byte-range positions for every markdown element — which is exactly what we need for TextTag application. Two compile errors, both trivial: `TagEnd::BlockQuote` needed `(_)` because it became a tuple variant in 0.12, and `anchor.deleted()` became `anchor.is_deleted()` in gtk4-rs. Fixed in under a minute.

### How image embedding composes with tag formatting

The buffer text (what gets saved to the server) never contains image widgets — `buffer.text(start, end, false)` excludes child anchors. So the note content is always clean markdown. The widgets are purely decorative. When a different note is selected, `set_text()` on the buffer wipes everything including anchor characters; the anchor objects in `image_anchors` become deleted (`.is_deleted()` returns true) and the cleanup code skips them gracefully.

### Where things stand

All three features compile and run. Typing `**word**` makes the whole span bold. `# Header` gets a larger font. Pressing Enter at the end of `- item` creates `- `. Local image paths embedded as `![alt](path)` show the image inline. The markdown text and the visual formatting coexist — the markup characters are visible but styled, which feels honest rather than hiding the syntax.

> 📝 *Write here: your reaction to typing a heading for the first time and seeing the font actually get bigger. Does live-preview feel different from the old plain-text editor? Is there anything that felt janky or not quite right?*

> 📝 *Write here: the experience of building a GTK4 app in Rust in 2026 compared to the C++ version. Same crate versions, same Blueprint file structure — but the Rust borrow checker catches the "win lives too long" lifetime bug that would have been a subtle crash in C++.*

---

## March 13, 2026 — Auth flow, offline mode, trash, and UX polish on Linux

### Starting point

The previous session left us with live markdown formatting and inline images working. The app could authenticate against the server, but the auth screen had a `Gtk.StackSwitcher` for navigating between Login, Register, and Recover — tabs that nobody found. This session focused on: making the auth flow discoverable, adding an offline mode, implementing the trash, and fixing image paste for screenshots.

### The StackSwitcher problem

The recover password flow was invisible. Users land on the Login page, see email and password fields, and a "Sign In" button. The tabs at the top say "Login", "Register", "Recover". In practice, nobody clicks the Recover tab — it's not where you look when you've forgotten your password. You look below the button.

The fix: remove the `Gtk.StackSwitcher` entirely. Add inline contextual links beneath each action button — "Create account" and "Forgot your password?" on the login page; "Already have an account? Sign in" on the register page; "Back to sign in" on the recover page. These are `flat`-styled `Gtk.Button`s that switch the `Gtk.Stack` page.

The wiring lives in `AmzAuthView::ObjectImpl::constructed()` — the view's own `constructed()` override handles its internal navigation, so the window doesn't need to know about it.

### The `Gtk.StackPage` name trap

This produced a subtle bug. The `auth_stack` used `Gtk.Box { name: "register"; }` as page children. In Blueprint, `name:` on a widget sets `GtkWidget.name` — the CSS name — not the `GtkStackPage` name that `set_visible_child_name()` looks up. The result was a "Child name 'register' not found in GtkStack" runtime warning and a no-op navigation.

The fix is `Gtk.StackPage { name: "register"; child: Gtk.Box { … }; }`. One wrapper per page. This is documented now in PROGRESS.md and VISION.md so future sessions don't have to rediscover it.

### The disabled button problem

A second auth bug: if the user tried to login with wrong credentials, the login handler called `set_sensitive_all(false)` (disabling all auth buttons while the request was in-flight). The request failed, `on_auth_error` fired, `set_sensitive_all(true)` re-enabled them. So far so good. But if the user clicked "Forgot your password?" *while the request was still in-flight*, they landed on the recover page with the button already greyed out, and it never re-enabled from their perspective.

The fix: connect to `auth_stack.connect_visible_child_notify`. Every time the page changes, re-enable all buttons and clear the error label. A request on one page cannot bleed into another page's state.

### The trash and the FK violation

Adding a trash/wastebasket feature seemed straightforward: move a trashed note to `folder_id = "__wastebasket__"`. The server immediately rejected this with a foreign key violation — `folder_id` is a real FK constraint to the `folders` table, and `__wastebasket__` doesn't exist there.

Option A: create a real "Wastebasket" folder in the DB. Rejected — it would appear in every user's folder list and require special-casing everywhere.

Option B: add a `deleted_at INTEGER` column to `notes`. Chosen. `PATCH /notes/:id/trash` sets `deleted_at = now()`; `PATCH /notes/:id/restore` sets it to NULL. `GET /notes` returns everything including soft-deleted notes, and clients use `deleted_at` to determine which notes belong in the trash.

The client maps this at the boundary: when loading from disk or applying a sync, any note with a non-null `deleted_at` gets `folder_id = "__wastebasket__"` as a local sentinel. The rest of the UI works against the sentinel — context menus check for it, the filter model uses it, the note row shows the folder name from it.

The DB migration uses `pragma_table_info` to check whether `deleted_at` already exists before running `ALTER TABLE`. Safe to re-run without versioning the schema.

### The welcome screen

Before this session, a user with no saved token landed on the auth screen. That's wrong for an offline-first app — you shouldn't need a server to use it.

The new first screen is a `Adw.StatusPage` with the app icon and two `Adw.ActionRow`s (GNOME HIG pattern for a choice between modes):
- **Use Offline** — notes saved locally, no account, no server. Sets `offline_mode = true` in `data.json` and immediately loads the note list.
- **Connect to Server** — navigates to the auth stack.

`offline_mode` is persisted to disk, so subsequent launches skip the welcome screen. The menu button respects the mode: offline users see "Return to Start" (no destructive confirmation — no data is lost by returning); authenticated users see "Sign Out" (destructive, clears local data).

The "Connect to Server" button on the welcome screen always resets the auth stack to the login page before navigating there, so you never land on the recover page by accident.

### Clipboard image paste

The existing image paste handler only handled `gdk::FileList` — files dragged or copied from a file manager. Screenshots via PrintScreen and images copied from web browsers place a `GdkTexture` on the clipboard, not a file list. The handler returned `false` for those, letting GTK's default paste handler run, which did nothing useful in a `GtkTextView`.

Added a second branch: if `clipboard.formats().contains_type(gdk::Texture::static_type())`, call `clipboard.read_texture_async()`. The callback receives `Result<Option<Texture>, Error>` — the `Option` is `None` when the clipboard had no image content (not an error). On success, save as PNG to `~/.local/share/amadeuz/images/{note_id}/` and insert the markdown reference.

### Where things stand

The Linux client and server are at feature parity for this milestone. The app handles the full user lifecycle: first-launch choice (offline vs. online), account creation with recovery codes, password recovery, note and folder management, live sync across devices, trash with soft delete, inline images via drag-and-drop or clipboard paste, and live markdown formatting. Everything compiles clean. `just dev` builds and runs in one command.

> 📝 *Write here: what it felt like to finally get the auth navigation working after the StackPage name bug. Was the fix satisfying or frustrating — a simple one-line change that required understanding an obscure GTK/Blueprint distinction?*

> 📝 *Write here: the offline mode decision — building software that doesn't require a server to be useful is a design principle, not just a feature. Did adding it feel like an afterthought or something that should have been there from day one?*

---

## March 14, 2026 — Rebuilding the macOS app: Apple Notes layout

### The decision

The macOS app was in `legacy-code/mac/` — a working Phase 2a implementation but using the old
single-note-per-websocket architecture and a simpler UI. The Linux client had moved ahead:
full auth, trash, offline mode, date-grouped note list, per-folder note counts, and a layout
that actually looks like a polished native app.

The goal for this session: bootstrap a fresh macOS client from scratch, using the Linux client
as the feature reference and the actual Apple Notes app as the visual reference. The `notes/mac/`
directory was empty (deleted files still in git staging).

> 📝 *Write here: was looking at the screenshot of Apple Notes and comparing it with the old mac
> app a bit of an embarrassment? The old version had a functional 3-column layout but the rows
> were rough — no date grouping, no note counts on folders, no trash. It worked but it didn't
> look like it belonged on a Mac.*

### What was rebuilt

All 11 source files were written fresh — none were carried over verbatim (though several are
close to their legacy equivalents):

- **Models.swift**: Added `deletedAt: Int64?` to `Note` and the `isTrashed` computed var. The
  rest of the type system stayed the same. The `deleted_at` field round-trips through
  `data.json` and the server API automatically via Codable.

- **APIClient.swift**: Added `trashNote(id:)` and `restoreNote(id:)`. Both call `makeRequest`
  with no body — `PATCH /notes/:id/trash` and `PATCH /notes/:id/restore` respectively. The
  existing `makeRequest` already had `body` as optional, so this required no plumbing changes.

- **NoteViewModel.swift**: Three new sentinel IDs (`allNotesID = "__all__"`, `trashID = "__trash__"`),
  three new methods (`trashNote`, `restoreNote`, `permanentlyDeleteNote`), and the key addition:
  `noteSections: [NoteSection]` — a computed property that groups `notesInSelectedFolder` into
  Today / Previous 7 Days / Previous 30 Days / Older using `Calendar`. Sections with no notes
  are filtered out. Also added `allNotesCount`, `trashCount`, `noteCount(for:)` for the sidebar
  badges.

- **ContentView.swift**: The biggest change. `FolderSidebar` now shows note count badges on
  every row using SwiftUI's `.badge()` modifier. The trash section is a `Section {}` at the
  bottom — no section header, just the "Recently Deleted" row. `NoteList` iterates `vm.noteSections`
  with `ForEach(section.notes)` inside each `Section(section.title)`. Context menus are
  context-aware: trash view shows Restore / Delete Permanently, normal view shows Move to Folder /
  Move to Trash. `NoteRow` is the most visually different from the legacy version: bold title,
  then date + preview on the same line with different text styles, then a small folder label.
  `NoteEditor` adds the centered date stamp at the top.

### The date formatting detail

Looking at the Apple Notes screenshot: note rows show "Thursday" for notes from earlier this week,
and "09/02/2026" for older notes. Getting this right required checking whether the note's date
falls within the current calendar week (via `dateComponents([.yearForWeekOfYear, .weekOfYear])`),
not just within 7 days. A note from last Sunday is "Last Week" in casual speech but technically
within 7 days — the week boundary feels more natural.

### Build result

`swift build` completed clean on the first attempt. No surprises — the Swift type system
caught everything at compile time, and all the async patterns from the legacy code carried over
unchanged. The debounce via `Publishers.CombineLatest` + `.debounce` is particularly clean
compared to what most platforms need to do for the same thing.

### Where things stand

The macOS client is now at full feature parity with the Linux client for this milestone: Apple
Notes-style layout, date-grouped note list, folder counts, trash with soft delete / restore /
permanent delete, inline images, Markdown styling, auth, offline-first sync. The matrix now
shows macOS ✅ for trash.

> 📝 *Write here: compare writing SwiftUI to the GTK4/Rust work. SwiftUI's `.badge()`,
> `NavigationSplitView`, and the `Section` type in `List` make the sidebar and grouped list
> almost embarrassingly easy to implement compared to the GTK equivalent. But the Markdown
> editor — NSTextView with custom attachment handling — is genuinely tricky and not something
> SwiftUI gives you for free.*

---

## March 14, 2026 — Rebuilding the iOS client from scratch

### Why the old one had to go

The old iOS app was a Phase 1-era codebase: single shared note, no auth, a simple
`URLSessionWebSocketTask` wrapper talking to the original single-note server. Since then,
the server grew user accounts, JWT, REST CRUD, folders, trash, per-note WebSockets, and
blob storage. The mac client caught up feature-for-feature. The old iOS app was just
stranded — not worth patching.

The decision was the same one made with the C++ Linux client before rewriting it in Rust:
preserve the old code in `legacy-code/ios/`, start clean, and take the mac client as
the specification. If it works on mac, it should work identically on iOS.

> 📝 *Write here: was there any hesitation about throwing away the old iOS code? What
> was the emotional or practical calculus? Did the legacy-code folder make it easier?*

### What "take mac as gospel" means in practice

Seven of the twelve files are straight copies — not adaptations, not ports, copies:
`Models.swift`, `APIClient.swift`, `KeychainStore.swift`, `SyncService.swift`,
`LocalStore.swift`, `BlobStore.swift`, `NoteViewModel.swift`. The entire sync engine,
auth logic, REST client, local persistence, blob queue, and per-note WebSocket are
100% shared between the two Apple platforms. Swift's ability to target both macOS and
iOS from the same source without conditional compilation was one of the original
reasons for choosing it, and it delivered exactly that here.

The only files that needed real work were the ones that touch UI or system APIs:
`NoteApp.swift` (trivial — drop the AppDelegate hack iOS doesn't need),
`AuthView.swift` (swap `NSPasteboard` for `UIPasteboard`),
`ContentView.swift` (drop `navigationSubtitle`, move Settings from editor toolbar
to sidebar), and `MarkdownEditor.swift` (the real work).

### Porting the Markdown editor: NSTextView → UITextView

The mac editor is built on `NSTextView` + `NSScrollView`. On iOS the equivalent is
`UITextView`, which already scrolls — no wrapper. The `NSTextStorage` / `NSLayoutManager`
layer that does all the markdown syntax highlighting is exactly the same on both platforms
(it's Foundation, not AppKit). So `applyMarkdownStyling()`, `applyLineStyle()`, and
`extractMarkdown()` translate almost character-for-character, just swapping:

- `NSFont` → `UIFont`
- `NSColor.labelColor` → `UIColor.label`
- `NSColor.tertiaryLabelColor` → `UIColor.tertiaryLabel`
- `NSImage` → `UIImage`

The trickier part was the Enter key. On macOS, `NSTextView.keyDown(with:)` intercepts
the Return key before the text system processes it. On iOS, there's no `keyDown` —
instead, `UITextView.insertText(_:)` is overridden to catch `"\n"` before calling
`super.insertText`. The list-continuation logic (smart bullets, ordered lists, empty
line exits) is the same algorithm; the intercept point is different.

Text replacement inside the Enter handler also differs. On macOS, `insertText(_:replacementRange:)`
takes an `NSRange`. On iOS, the cleanest path is `UITextInput.replace(_:withText:)` which
takes a `UITextRange` and properly fires all delegate callbacks. Converting an `NSRange`
to a `UITextRange` requires chaining `position(from:offset:)` and `textRange(from:to:)` —
a minor but slightly clunky UIKit ritual.

### Image insertion

The mac editor supports drag & drop and paste. iOS doesn't have drag & drop in the same
sense, so the approach was:

1. **Paste**: override `UITextView.paste(_:)`, check `UIPasteboard.general.image`, convert
   to PNG data, call `insertBlobData`. Simple.

2. **PhotosPicker**: a SwiftUI `PhotosPicker` button (`photo.badge.plus`) in the note
   editor toolbar. Because `MarkdownEditor` is a `UIViewRepresentable`, passing the selected
   image data into the UIKit layer is done via a `@Binding var pendingImageData: Data?`.
   `updateUIView` watches for a non-nil value and calls `tv.insertBlobData(data)` on the
   underlying `MarkdownTextView`. It's not the most elegant bridging pattern, but it's
   clean enough and avoids the alternative (notifications, custom delegates, or routing
   through the ViewModel).

### Where things stand

The iOS client is now at full feature parity with the macOS client: auth, folders, notes,
trash, date-grouped list, inline images, Markdown styling, offline-first sync, per-note
WebSocket. The matrix is updated. The old app is in `legacy-code/ios/`.

> 📝 *Write here: what does it feel like to have a phone client again — and one that
> actually syncs with the server you built yourself? The first time a note you type on
> your phone appears on your Mac without any manual action — is that the moment this stops
> feeling like a coding exercise and starts feeling like a real product?*


---

## March 14, 2026 — Bringing Windows to parity

### The problem

After several focused sessions on macOS, Linux, and iOS, the Windows client was left behind.
The server had moved on completely: it dropped the custom shared-WebSocket protocol in favour
of standard REST + per-note WebSocket rooms. Every other client had already been rewritten
against the new server. Windows hadn't. Trying to run the Windows app against the current
server would have resulted in silent failure — the old `/ws` endpoint simply doesn't exist
anymore.

The gap was wider than it looked from the outside. It wasn't just "add auth". The entire
sync layer was built around a protocol that no longer exists. `SyncService.cs` connected
to a single WebSocket endpoint and received typed bus messages (`create_folder`, `update_note`,
`init`). `NotesViewModel.cs` was wired to those message types. `Models.cs` had a `WsMessage`
class that matched the old format. None of it was compatible with the new server.

The decision was to do a clean rewrite of the sync layer rather than patch around it.

### What was rewritten

**`ApiClient.cs`** (new file) — A dedicated REST client that mirrors `APIClient.swift` on
macOS/iOS. All HTTP interactions live here: auth (login, register, recover), folder CRUD,
note CRUD (including trash/restore/move). `PasswordVault` JWT storage is also here —
Windows Credential Manager, the same role as Keychain on Apple platforms and libsecret
on Linux. One small convenience method: `NormaliseUrl()` accepts `ws://`, `http://`, or
bare hostnames and returns a clean `http://` base URL — because users will inevitably
paste WebSocket URLs from older sessions.

**`SyncService.cs`** (rewritten) — The class name stayed the same but the internals are
completely different. Instead of connecting to a single bus and dispatching typed messages
for all notes and folders, it opens one `ClientWebSocket` per note at
`GET /notes/:id/ws?token=<jwt>`. The connection opens when a note is selected, closes when
a different note is selected. The class is now `IDisposable` — the ViewModel creates and
disposes instances as the user navigates.

**`NotesViewModel.cs`** (full rewrite) — The biggest change. The new version is built around
`FullSyncAsync()`: on connect, parallel REST calls to `GET /folders` and `GET /notes`,
merge by last-write-wins, push any locally-created or locally-newer items. This is the
same offline-first pattern as every other client. The old version had a `HandleInit` method
that explicitly dropped local-only notes with a comment "Local-only (offline-created, no
server ID): dropped". That was never the right behaviour — it was a shortcut that worked
when the app was online-only but breaks offline use. Fixed.

**`Models.cs`** — `WsMessage` is gone. Added `NoteWsMessage` (per-note WS: `type`, `title`,
`content`, `updated_at`), `AuthResponse` (`token`, `recovery_code`), `FolderItem.IsWastebasket`,
and `Note.DeletedAt` (nullable `long` with `INotifyPropertyChanged`).

**`MainWindow.xaml` and `MainWindow.xaml.cs`** — Auth overlay added as a second layer in
the root Grid. Auth card with Log In / Register / Recover mode tabs, server URL field,
email, password, recovery code, and new password fields. Sign Out button in the status bar.
Search box in the note list column. Context menus are now context-sensitive: in the
wastebasket view, right-click shows Restore and Delete Permanently; in any other view, it
shows Move to Trash and a Move to Folder submenu.

### The recovery code timing problem

Registration shows a recovery code that the user needs to save. The first design put the
code in an inline panel inside the auth overlay. It was never visible.

The sequence: user submits register → server returns token + recovery code → ViewModel
enqueues two `DispatcherQueue.TryEnqueue` calls: first `StateChanged` (which triggers the
window to hide the auth overlay), then `RecoveryCode` (which would trigger the panel to
appear). By the time the second callback ran, the auth overlay was already `Visibility.Collapsed`
and the panel was hidden with it.

Fix: make the recovery code a `ContentDialog` created entirely in code-behind. The dialog
is shown after the auth overlay has hidden and the main view has appeared. A copy button
lets the user get the code into their clipboard without manually selecting the text.

### The `deleted_at` / `folder_id` mismatch

The server represents trash state as `deleted_at: <unix ms>` on a note, with `folder_id`
left as its original value (or null). The client uses `folder_id` as the single routing key
for the note list — which virtual folder bucket a note belongs to.

The fix is `NormaliseNote()`, called at every point notes enter the ViewModel from outside
(REST sync, local load, WebSocket init). If `deleted_at` has a value, it sets
`folder_id = "__wastebasket__"`. This is the same pattern the Linux and macOS clients use
to map the server's semantic into a UI-friendly representation.

### Where things stand

The Windows client is now at feature parity with macOS for the core experience:
auth (login, register, recover with recovery codes), JWT in Windows Credential Manager,
folders (create, rename, delete), notes (create, edit, delete, move between folders),
trash (soft delete, restore, permanent delete), search, and live sync via per-note
WebSocket.

Still missing compared to macOS and Linux: Markdown rich text, inline images, and the
folder label shown in note list rows. Those are the next steps.

> 📝 *Write here: how does it feel to finally have the Windows client running against the
> same server as everything else? This is the platform you use every day — does opening the
> app and seeing your notes appear feel different than testing on macOS?*

---

## March 14, 2026 — Windows catches up: Markdown, images, single-body editor, folder labels

### The remaining gap

After the sync rewrite session, the Windows client had parity with macOS on structure —
auth, folders, notes, trash, search, live WebSocket sync — but the note editor was a plain
text box. No formatting. No images. And the note title lived in a separate `TextBox` above
the editor, which none of the other platforms do anymore.

The five things that needed fixing were:

1. Use the first line as the title, same as macOS and iOS.
2. Show the folder name in each note list row.
3. Make the Wastebasket button look like the New Folder button — icon on the left, anchored
   at the bottom of the sidebar.
4. Markdown formatting.
5. Image paste.

All five were done in a single session.

### First line as title

macOS introduced this idea back when it moved from a two-field editor to a single `NSTextView`.
The note has a `title` and a `content` field internally, but the editor shows them concatenated:
`title + "\n" + content`. When the user edits, `SplitBody()` extracts the title back out at
the first newline.

The Windows version had never adopted this. It had a dedicated `TextBox` with placeholder
"Title" and a separate `TextBox` for the body. That works, but it's visually different from
every other client and adds a field the user has to consciously click into.

Fix: remove the title `TextBox`, replace with a single `RichEditBox` that holds the full
body. `BodyFrom(note)` concatenates; `SplitBody(body)` splits at the first `\n`. `FlushNote`
calls `SplitBody` to extract title and content before saving. Identical to how the macOS
and iOS apps work.

### Markdown with `RichEditBox`

The reason to use `RichEditBox` (rather than keeping a plain `TextBox`) is that Markdown
styling requires per-character formatting — different font sizes for headings, bold weight
for `**spans**`, italic for `_spans_`, strikethrough for `~~spans~~`. `TextBox` is uniform
formatting only.

`RichEditBox` exposes `ITextDocument.GetRange(start, end)` which returns an `ITextRange`.
Each range has `CharacterFormat` with properties for `Size`, `Bold`, `Italic`,
`Strikethrough`, and `ForegroundColor`. That's everything needed for Markdown rendering.

The implementation follows the same visual rules as macOS and Linux:
- First line (title): 20 pt, bold, regardless of its content
- `# Heading`: 22 pt bold, marker dimmed to grey
- `## Heading`: 18 pt bold, marker dimmed
- `### Heading`: 15 pt bold, marker dimmed
- `- item` / `* item`: marker dimmed
- `- [ ]`: marker dimmed
- `- [x]`: marker dimmed, `[x]` green, rest of line struck-through
- `**bold**`: bold weight
- `_italic_`: italic
- `~~strikethrough~~`: strikethrough

`ApplyMarkdownFormatting()` runs in two passes per line: first the block-level rules
(heading or bullet or first-line), then inline spans via `ApplyInlineSpan()` which scans
for paired markers.

Two guard flags prevent the most painful WinUI pitfall: `_suppressEditorChanged` stops
`TextChanged` from firing during programmatic `SetText`; `_applyingFormat` stops
`TextChanged` from firing when `CharacterFormat` assignments trigger the event.
Without both, you get infinite loops. The formatter is debounced at 120 ms
(a separate timer from the 500 ms save debounce) to avoid per-keystroke layout invalidation.

`RichEditBox` uses `\r` as paragraph separator internally, and `GetText` always appends a
trailing `\r\0`. `GetBody()` strips exactly one trailing `\0` and one trailing `\r`, then
converts all remaining `\r` to `\n` before returning. This is the kind of thing that
takes an hour to figure out the first time.

### Folder labels in note rows

The note model has a `FolderId` field but no `FolderName`. The ViewModel knows both, but
the DataTemplate in the note `ListView` only has access to `Note` properties.

The fix: add `FolderName` as a `[JsonIgnore]` property on `Note` that implements
`INotifyPropertyChanged`. The ViewModel calls `UpdateNoteFolder(note)` (and
`UpdateNoteFolderNames()` for all notes) after every sync, folder rename, and note move.
When `FolderName` changes, it fires the `FolderLabel` property change too — `FolderLabel`
is a computed string that returns "—" for unfiled notes. The DataTemplate binds to
`FolderLabel` with `Mode=OneWay`, so the row refreshes automatically whenever the folder
name changes.

### Wastebasket button

Previously the Wastebasket was in the `FolderItems` `ObservableCollection`, rendered by
the same `ListView` as real folders. This caused subtle problems: the Wastebasket sentinel
would be included in "Move to Folder" submenus, renamed if a right-click happened to
land on it, and its selection state was erased whenever `RebuildFolderItems()` ran.

The new design removes it from the collection entirely and places it as a XAML `Button` in
Row 1 of the sidebar's three-row Grid — above the "New Folder" button, below the folder
ListView. It matches the "New Folder" button exactly: left-aligned icon (trash glyph),
text label, same padding and border. A `_wastebasketSelected` bool tracks whether it's the
active view; when it is, `AccentButtonStyle` is applied. When the user clicks a real folder
in the `ListView`, the `SelectionChanged` handler clears `_wastebasketSelected` and removes
the accent style.

### Inline images

`RichEditBox` has a built-in Ctrl+V that pastes bitmaps as OLE objects. OLE objects are
opaque to the `ITextDocument` API — there is no way to get the pixel data back out to
serialize for sync. If we let the default paste run, images become unreadable blobs
embedded in the document.

The fix: intercept Ctrl+V in `NoteRichEditBox_KeyDown`. Check whether the clipboard
contains a bitmap. If so, set `e.Handled = true` (suppresses the default paste) and
handle it manually: decode the clipboard bitmap with `BitmapDecoder`, re-encode as PNG
with `BitmapEncoder`, save the bytes to `%APPDATA%\amadeuz\blobs\{id}.png`, queue a
background `PUT /blobs/{id}` upload, then insert `![](amadeuz://blob/{id})` at the
cursor position as plain text. This is exactly the same pattern as macOS and Linux.

The Ctrl key detection uses `Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control)`.
WinUI 3 does not expose a simple `Keyboard.IsKeyDown()` helper the way WPF does — you
have to ask the input subsystem directly.

### Where things stand

The Windows client now has full visual and functional parity with macOS and Linux for
the everyday note-taking experience. All five gaps are closed. The feature matrix is green
across the board for Windows on everything except the offline blob queue
(images inserted while offline are not re-uploaded on reconnect — the macOS/iOS
`pending_blobs.json` pattern hasn't been ported yet).

> 📝 *Write here: the first time you pasted a screenshot into a Windows note and watched it
> sync to your phone — what did that feel like? This is the moment where it stops being
> "an app I'm building" and starts being "a tool I actually use". Is there a specific note
> or image that was the first real use, not a test?*

---

## March 15, 2026 — Throwing out WinUI 3 and starting over with WPF

### The problem with WinUI 3

The WinUI 3 client worked. All the features were there. But the build and packaging story
was a mess, and it kept getting messier.

The list of papercuts that accumulated:

- `WindowsAppSDKSelfContained=true` bundles WinAppSDK native DLLs. Those DLLs are
  version-locked to a specific Windows build and crash on Windows Insider Preview with a
  `CoreMessagingXP.dll` version mismatch. So self-contained publish — the whole point of
  distributing an app without a runtime install — didn't actually work.
- Without `SelfContained`, the publish output was missing resource files. A custom MSBuild
  target (`CopyPriFilesToPublish`) was needed to copy `*.pri` files to the output folder,
  because the normal WinUI build pipeline doesn't do this when self-containment is off.
- `dotnet publish` fails entirely on WinUI 3 PRI generation. The `ExpandPriContent` MSBuild
  task requires VS-installed tools not present in the dotnet SDK. Publish requires
  `MSBuild.exe` from a full Visual Studio installation.
- `WindowsAppSdkBootstrapInitialize=true` must be set explicitly in the csproj. Without it,
  the app crashes silently before XAML loads — `STATUS_FAIL_FAST_EXCEPTION` with no useful
  error message.
- The target machine needs the Windows App Runtime 1.8 installed separately. The app shows
  a download dialog on first run on a new machine, which is fine for personal use but is a
  friction point for sharing.

Each of these had a workaround. But the workarounds were fragile, each one was specific to
a particular combination of SDK and Windows build version, and debugging any one of them
consumed several hours without producing insight that would transfer to any other problem.

> 📝 *Write here: what was the moment you decided to stop patching and start over?
> Was there a specific build failure that pushed you over the edge, or was it the
> accumulated weight of a dozen small annoyances?*

### The rewrite

WPF with .NET 9. `dotnet build`. It works. That's it.

No runtime install needed on the target machine with `--self-contained`. No custom MSBuild
targets. No `pri` file archaeology. No bootstrap init ceremony. The same `dotnet publish`
command that works for every other .NET app works here.

The architecture is identical to the WinUI 3 client: same file structure, same API surface,
same sync logic, same MVVM pattern. The rewrite was mechanical — swapping WinUI 3 controls
for WPF equivalents, replacing WinRT APIs with BCL equivalents, and updating the data
binding syntax. The `ObservableCollection` diffs, the debounce timers, the per-note
WebSocket lifecycle — all of it moved over unchanged.

The only genuinely new decisions were about the styling library and the credential store.

### The styling library hunt

WPF out of the box looks like 2010. For a notes app that's supposed to feel native on
Windows 11, that's not acceptable. The obvious choice is `Wpf.Ui` by lepoco — it's the
most-cited modern WPF styling library, has good documentation, and is actively maintained.

The problem: the NuGet package ID is `WPF.UI`. That ID is squatted by an unrelated
Chinese package — `WPF.UI 3.1.0`, targeting net40, last updated years ago. When you run
`dotnet add package Wpf.Ui`, you get the squatter. There is no `Wpf.Ui` (lowercase) on
nuget.org that resolves to lepoco's library from a standard package add. You can work around
it by specifying an exact version or a specific source, but it's a trap that will catch
anyone following the standard docs.

ModernWpfUI (0.9.6) installs cleanly and provides everything needed:
- `ui:WindowHelper.UseModernWindowStyle="True"` — Fluent chrome on the window
- `AccentButtonStyle` and `TextBlockButtonStyle` — the two button styles used in the app
- `ui:ControlHelper.PlaceholderText` — attached property for placeholder text on inputs
- Automatic dark/light theme following (no code needed)

It's not as actively maintained as lepoco's library, but for the feature set this app needs,
it's sufficient. If a migration to `Wpf.Ui` becomes necessary later (via explicit source or
package ID workaround), the controls map 1:1.

### The credential store

WinUI 3 used `Windows.Security.Credentials.PasswordVault` (Windows Credential Manager) via
WinRT. In plain WPF, WinRT APIs are not available without P/Invoke interop. For a credential
store, that complexity is unnecessary.

DPAPI (`System.Security.Cryptography.ProtectedData`) is available in the .NET BCL. It
encrypts with the current user's Windows login credentials — equivalent security for the
single-user use case. The token is stored as an encrypted binary at
`%APPDATA%\amadeuz\token.dat`. `Protect` on write, `Unprotect` on read, `File.Delete` on
sign out. Twenty lines of code, no interop, no third-party library.

### Where things stand

The WPF client builds with `dotnet build`. It has feature parity with the WinUI 3 client
it replaces. The old WinUI 3 code lives in `notes/windows/` and is preserved for reference
if any of the WinUI 3-specific patterns (Mica backdrop, `RichEditBox` formatting) are
needed as a reference point.

The offline blob queue is still the one missing feature — images inserted while offline are
not re-uploaded on reconnect. That gap exists on Linux too and follows the `pending_blobs.json`
pattern already implemented on macOS and iOS.

> 📝 *Write here: does the WPF app feel different to use compared to the WinUI 3 one?
> Is there anything that looks or behaves noticeably differently to a user, or is it
> indistinguishable? The WinUI 3 Mica backdrop is gone — does that matter?*
