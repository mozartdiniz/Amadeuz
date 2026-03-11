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

The other platforms (Windows, Linux, iOS, Android) still use a plain `TextEditor`/`GtkTextView`/`UITextView` with no image or Markdown support. Images and Markdown are currently macOS-only.
