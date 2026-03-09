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
