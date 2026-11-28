# Utre: project guide and handoff

A personal, local-first TickTick replacement. The pieces:
- An Android app with a home-screen widget.
- A macOS and Arch Linux (Hyprland) desktop app.
- Peer-to-peer sync between devices, with no server.
- A triage Inbox and first-class nested tags.
- A Google Calendar view: events come in, timed tasks go out.

**The plan is the source of truth.** It lives in [`plan/`](plan/) as Obsidian notes. Start at `plan/00 Overview.md`.

| Read this | For |
|---|---|
| `plan/01 Requirements & Scope.md` | What is in and out. **Don't add features the user declined** (kanban, pomodoro, habits, matrix, natural-language quick add, …) |
| `plan/02 Tech Stack Options.md` | Decision: **Rust core + Flutter UI + a Kotlin Glance widget**, plus the confirmed library versions |
| `plan/03 Architecture.md`, `plan/04 Data Model.md` | The core design. The Inbox is a *derived* view, not a stored flag |
| `plan/10 Roadmap.md` | Milestones M0–M6 and what is ticked |
| `plan/11 Open Questions.md` | Undecided items. Ask the user before deciding them |
| `plan/13 M0 Spike Log.md` | **Spike results and every gotcha found so far. Read before touching Android or Gradle.** |
| `plan/14 M1 Core Log.md` | How the core is built (op-log, HLC, filters, recurrence) and where it differs from plan/04 and plan/05 |
| `plan/16 M3 Sync Log.md` | M3 sync protocol, pairing, what's next |
| `plan/17 M4 Android Log.md` | M4: phone sync, reminders, background sync, CI release, emulator results |
| `plan/18 M5 Calendar Log.md` | M5 plan: M5a local calendar (decisions from the 2026-11-25 interview, slices), then M5b Google |
| `plan/15 M2 Desktop Log.md` | **M2 plan, slices and the bridge contract.** The socket API must copy the bridge API |

The UI style is decided: **Soft** (rounded cards, teal placeholder accent, Manrope headings) **with Material outlined tag chips**. The mockups are in `plan/ui-styles/styles.html`.

> **Renamed to Utre (2026-11-26).** The handoff below was written as "Tick Alt" and is kept as written:
> read `tt` as `utre`, `tick-alt` as `utre`, `dev.tickalt.*` as `dev.utre.*`. The old-to-new table is in
> `plan/00 Overview.md`. The sync ALPN (`tickalt/sync/1`) and the `tt1`/`tt2` ticket prefixes keep their old
> spelling on purpose (wire protocol).

## Latest session (2026-11-26, Arch laptop, orchestrated multi-agent run; released as v0.8.0 to v0.8.7)

Parallel agents in worktrees, each change a PR with green CI (PRs 19 to 44, all merged; nothing open).
- **v0.8.13 (2026-11-28):** fresh Android installs can start sync again (PR 72: SELinux denies hard links, the device key now falls back to renameat2/rename; plan/16). Every fresh install from v0.8.0 to v0.8.12 was stuck at 'Starting sync'.
- **v0.8.12 (2026-11-28):** the first APK signed with the release key (cert SHA-256 60:98:6D:…:F5:C3, `packaging/android/release-cert.sha256`, enforced by `scripts/build-apk.sh`); phones on older debug-signed builds need one uninstall and reinstall (sync first, then pair again and sign in to Google). Also CI path filters, release-build caching, the u-tick logo on the README and website. `main` requires PRs.
- **CI:** hardened, with blocking rustfmt, clippy and shellcheck, 26 Linux golden tests (`app/test/golden/`), and a non-required Linux integration job under xvfb.
- **UI:** a date and time picker (dialog on desktop, bottom sheet on the phone), Rust, Android and Flutter review fixes, `start_at` and the keyboard focus ring, the new **u-tick icon** and tray icon (PR 33; plan/20 round 8 and 9), and calendar bugs the goldens found (PR 34: all-day in Day/Month, "+N", compact tiles, "1 days ago", sheet tint).
- **Core:** the `network_sim` flake is explained and fixed (PR 32, a test bug: a ring plus compaction can strand a superseded op, so version vectors differ while state is identical; plan/16).
- **Google calendars, choose which are visible** (PR 35, **schema v10**, per device, plan/19): desktop toolbar "Calendars" menu, phone "Google calendars..." sheet. An older build refuses a v10 DB.
- **v0.8.2:** automatic reminder at due time (PR 36; plan/14): setting a time on a task with no reminders adds one at that time. Unverified on a real phone alarm.
- **v0.8.3:** dragging a task onto the calendar adds the same reminder (at the block's start), and dragging it out or onto the all-day row removes it if it is the only one (PR 37; plan/14).
- **v0.8.5:** the Linux app **prefers XWayland** (typing was laggy on native Wayland in Flutter's GTK3 embedder, smooth under X11; `UTRE_WAYLAND=1` forces native; lowercase WM_CLASS `dev.utre.utre`; PR 41; plan/20 round 11). No phone changes.
- **v0.8.6:** click a **Google event** to see its details (title, time, calendar, location, description) and **Open in Google Calendar** or copy the link (PR 42; plan/19; **schema v11**, per device, an older build refuses it; the Android intent and the Calendar app claiming the link are unverified). `cargo test -p utre-api` now works standalone.
- **v0.8.7:** a **Duration** field on tasks (detail row and picker, card chip, calendar blocks, `utre add|edit --duration`; PR 44; plan/20 round 12; no schema change, still **v11**) and the Unscheduled panel lists only undated tasks with Show in Calendar on, a dropped card leaving it at once (PR 43; plan/18). Unverified on the phone, the Mac and real Google. **Agent guard** now also blocks `pkill`/`killall`.
- **Packaging (2026-11-27, PRs 50 to 52, no app release):** **MIT LICENSE** (Copyright (c) 2026 Boris Belev), a root **README.md** with install steps, and a **signed pacman repo** (`[utre]` on the `arch-repo` GitHub release, `utre-bin` 0.8.10, signing key `8D9A9E54…43ADD341` in `~/.gnupg`). **This laptop now runs the package**: `/usr/bin/utre-daemon` under `/usr/lib/systemd/user/utre.service`, the app from `/usr/lib/utre`; updates come from the user's `sudo pacman -Syu`, and `update-desktop.sh` refuses here. The AUR (`scripts/aur-publish.sh`) waits for the user's account (registration was closed). A Homebrew cask (own tap) was proposed; signing (Apple Developer ID or unsigned) is the user's call. Details in plan/08 "Packaging". **Released as v0.8.11 with auto-update on the Mac and the phone (PRs 53 to 57):** the Mac's daemon updates itself from GitHub releases (OpenPGP-checked), the phone has an in-app updater (plan/17). **The Mac still needs the one-time move from Tick Alt 0.3.0:** download the zip, right-click and Open, run `Utre.app/Contents/Resources/install-launch-agent.sh`, then remove the old `dev.tickalt.daemon` LaunchAgent and `Tick Alt.app`. **The phone needs one manual install of 0.8.11**, after which it updates itself.
- **v0.8.8 (2026-11-27):** **nesting tags** in the app (PR 46; plan/15 "Nesting tags"): while arranging, drop a tag on another's middle to nest it, on an edge to place it (drag left/right for the level), Alt+Right/Left; a **+** on Tags (takes `parent/child`) and **New sub-tag** in the tag dialog; parent tags **fold** with a chevron (per device, `desk.json` `collapsed_tags`). Core `Store::nest_tag`, API `NestTag`; no schema change (still v11). Unverified by eye and on the phone.
- **v0.8.4:** arrange **lists and filters** in the sidebar like tags (PR 38; plan/15, `utre lists order`, `utre filter order`; unverified on the phone); checkboxes centred on the title's first line and title/notes autosave debounced, with fewer rebuilds while typing (PR 40; plan/20 round 10; the user's typing lag was not reproduced, suspects are GPU raster or GTK-on-Wayland frame pacing). **Agent guard** (PR 39): a PreToolUse hook blocks endless wait loops, plus `scripts/agent-reap.sh`. `agent-worktree.sh` now works from inside a worktree.
- **User data change (by hand, not in code):** the Work list's 26 tasks were moved to a new `work` tag (icon ✈️) and the Work list was deleted.
- **Running here:** `utre.service` runs this checkout's `target/fast/utre-daemon` on `utre.db` (schema v11; v10 backup `~/.local/share/utre/backups/utre-v10-pre-v11-*.db`); the daemon and desktop app were rebuilt at v0.8.7. **Phone:** v0.8.7 pre-release APK on GitHub, not installed yet (it upgrades its DB to v11 on first start; same debug key, updates in place). **The Mac is still on Tick Alt 0.3.0** (see v0.7.3 notes below: new app, remove the old LaunchAgent, `scripts/build-macos.sh`, `scripts/install-launch-agent.sh`).
- **Open items:**
  - Never tried on real hardware: the date picker sheet, the new launcher, themed and status-bar icons, the tray icon in waybar, the macOS menu-bar icon (unbuilt), the Google calendar picker with a real account, stale-reminder clearing across devices, and earlier items (widget Reconfigure and midnight refresh, tasks out to real Google, 2 Days and Month on the phone).
  - Waiting on the user: GitHub settings, the plan/11 decisions (including the review recommendations), the optional CI signing secrets (plan/10 'Move all builds to CI'), Google consent screen "In production" and app verification, the Mac switch-over.
- **Lessons:** agents occasionally leave test daemons running, so check `pgrep -af utre-daemon` and each one's `UTRE_DB` (in `/proc/<pid>/environ`) before a release; `gh pr merge --delete-branch` also removes the agent's worktree; `update-desktop.sh` backs up only `utre.db` (use `sqlite3 ... ".backup"` by hand before a schema change); test launches of the app need `UTRE_DB` and `UTRE_SOCKET`.

## Previous session (2026-11-26 night, released as v0.7.3)

Work was done by parallel agents in worktrees (CLAUDE.md "Multi-agent workflow"), each change a PR with green CI, merged by the integrator (the session the user talks to). PRs 1 to 18 are all merged; nothing is open.
- **CI** (`ci.yml`; caches saved only from `main`) and the multi-agent workflow. **network_sim fixed** (PRs 2, 8): after `compact()` a replica remembers the HLC of dropped ops, so version vectors may differ in a counter or even a whole device; state equality and "nothing owed" are still asserted.
- **Renamed to Utre** (PR 3): CLI `utre`, crates `utre-*`, `dev.utre.utre`, `utre.service`, data in `~/.local/share/utre` (migrated by the daemon on first start). Old-to-new table in `plan/00 Overview.md`.
- **Core:** subtask cycles resolve convergently (PR 4, schema **v9**, `task.parent_raw` plus a derived effective parent).
- **Google (plan/19):** tasks out (PR 5: off until switched on per device, repeats go as a window of single events) and events on the phone (PR 7). **Phone sign-in works on a real phone (v0.7.1)**, using an Android OAuth client in the user's Cloud project (client id in the git-ignored `app/android/google.properties`; it is bound to package `dev.utre.utre` and the Arch laptop's debug key SHA-1, so a release key needs its own client). Tasks out has only run against a fake Google.
- **Widget** (PR 6): source picker, scrolling list, **+** quick add, midnight refresh; a row tap opens the task.
- **UI** (PRs 9 to 18, `plan/20 UI Iteration Log.md`): task cards with a facts line, due-date groups, segmented priority, 48 px touch targets, light-theme contrast (`UTRE_THEME=light|dark` forces a theme for QA), phone/tablet fixes, softer dialogs, subtle motion, Month grid ending above the add button, a **2 Days** view on the phone.
- **Running here:** `utre.service` runs this checkout's `target/fast/utre-daemon` on `utre.db` (schema v9); the release desktop app was rebuilt from `main` at v0.7.1 (UI only since). Old DB backups: `~/.local/share/tick-alt/backups/tt-v8-pre-utre-*.db`. Phone and Mac are still listed as peers (same sync node id). `scripts/update-desktop.sh` rebuilds and restarts; after UI-only releases (0.7.2, 0.7.3) the desktop app was not rebuilt.
- **Phone:** v0.7.3 pre-release on GitHub (`utre-0.7.3-arm64.apk`, same debug key, updates in place from v0.7.0 on). **The Mac is still on Tick Alt 0.3.0:** `Utre.app` is a new app (remove the old LaunchAgent `dev.tickalt.daemon` and `Tick Alt.app`, then `scripts/build-macos.sh` and `scripts/install-launch-agent.sh`).
- **Open items:**
  - Never tried on real hardware: widget Reconfigure menu, the midnight refresh firing, the 48 px touch layout, the 2 Days and Month changes on the phone, tasks out to a real Google account (and two devices pushing).
  - Not seen visually: calendar current-time line, done and overlap states, hover/drag affordances on desktop, the Devices page with real peers, Google signed-in states on desktop.
  - Google: put the consent screen **In production** (else refresh tokens last 7 days) and decide on app verification (plan/19 "Open"); tokens still in a file, not the keychain.
  - Needs the user (plan/11): a release key plus the 4 CI secrets (plan/07), a LICENSE name, installing the PKGBUILD, M6 switch-over (TickTick import exists: `utre import ticktick`).
  - Small: `resolve_parents` re-reads all task links on every parent op (fine at this size); the subagent worktrees under `.claude/worktrees/` can be removed (`git worktree prune`).
- **Lessons:** Auto-fix events were unreliable, so wait with `gh pr checks <n> --watch`; `update-desktop.sh` backs up only `utre.db`, so back up by hand before any data-path change; a test launch of the app **must** set `UTRE_DB` and `UTRE_SOCKET`, or it opens the real DB (it happened once, no damage).

## Where things stand (handoff, 2026-11-25 evening: the Arch laptop)

**Latest session (Arch laptop, 2026-11-25 evening; released as v0.4.0, v0.4.1, v0.5.0 to v0.5.4; everything pushed):**
- **v0.5.0, sorting views** (plan/15 "Sorting views"): a sort menu in Inbox, list, tag and filter views (Due date, Priority, Title, Newest, Oldest, **Manual** with drag to reorder). **Since v0.5.3 one sort per device applies to every view** (`Store::device_sort`, `meta.sort`, never synced; the builder's filter sort is gone); schema v7's `list.sort`/`tag.sort` and `filter.sort` stay but are unused. Manual = the task's `sort_key` via `Store::place_task`. `tt sort`, `tt order`. The daemon here runs v7 (v6 backup in `~/.local/share/tick-alt/backups/tt-v6-2026-11-25.db`); the phone and Mac ignore sort changes until updated.
- **Arrange tags in the sidebar** (plan/15 "Arranging tags"): the ⇅ button on the Tags header, drag handles, a tag moves with its subtree among its siblings. Core `Store::place_tag` with fractional `sort_key`s (`core/src/order.rs`); `tt tags order <tag> [--before <tag>]`.
- **Resizable detail pane** (drag the line before it; double-click resets; per device in `desk.json`), a **Parent** picker plus "↑ parent" link in the detail (plan/15), and **Show in Calendar** per task (`task.in_calendar`, **schema v6**; plan/18; v0.4.1 shows the switch only on tasks with a date). `tt edit <id> --calendar false`.
- **Running here:** the `tick-alt` daemon and the release desktop app were rebuilt from `main` at v0.5.3 and restarted (DB v7); the desktop app again at v0.5.4 (UI only, the daemon's code is unchanged). The app and daemon must come from the same build.
- **Phone:** pre-release **v0.5.4** APK on GitHub; not installed on the phone yet (it upgrades its DB to v7 on first start). **The Mac is still on 0.3.0:** it logs but ignores Show in Calendar changes until it's rebuilt (the v6 migration then applies them).
- ⚠️ **`cargo test --workspace` has one failure: `network_sim`** (`replicas_meet_after_lossy_partitioned_syncs`). It predates this work; details and the saved seed in plan/16 "Next". The seed file `core/tests/network_sim.proptest-regressions` is untracked on this laptop. Everything else passes (18 desktop integration tests too), and clippy is clean. The v0.4.0 release build was smoke-tested on the x86_64 emulator (a throwaway `--target-platform android-x64` build): it starts with no crash.
- `tt sync peers` here shows the phone and **Boris's MacBook Pro**. The 09:14 "Reminder test from the laptop" check in `tt completed` is still open.


**Done** (details in the logs named in the table above):
- **M0**: stack and UI style decided; Spikes 1, 3, 4, 5 pass, on macOS too since M2 slice 5.
- **M1**: the core (op-log, HLC, filters, recurrence, reminders) and the full `tt` CLI. ⏸ The user hasn't yet "managed tasks from the terminal for real" (the M1 exit).
- **M2** (desktop, `plan/15`): the three-pane Flutter app, `tt-daemon` owning the DB over `$XDG_RUNTIME_DIR/tick-alt.sock`, the systemd user unit, the filter builder and edit dialogs, the PKGBUILD (`packaging/arch`, test-built, not installed). **Slice 5, the Mac (2026-11-25):** the daemon runs there with a menu-bar icon and `UNUserNotificationCenter` reminders, `scripts/build-macos.sh` makes `Tick Alt.app` (daemon bundled inside) and a DMG, and there's a LaunchAgent script.
- **M3** (sync, `plan/16`): protocol 2 over iroh (tt2 pairing tickets, trusted peers, trust sharing, removal spreading), compaction (schema v5; **v6** adds `task.in_calendar`), the network simulation test. The daemon runs the sync node with auto-sync, tray status and Sync now.
- **M4** (Android, `plan/17`): in-process phone sync (`app/rust/src/phone.rs` + shared `tt_sync::service::SyncService`), QR pairing, AlarmManager reminders (Done / Snooze 10 min / 1 h), WorkManager background sync (15 min, plus expedited after outside edits), a share target, the CI release workflow. Tested on the emulator.
- **M5a** (local calendar, `plan/18`): Day/Week/Month on real tasks in the Soft style, Unscheduled panel (desktop) and bottom drawer (phone) with drag in and out, `placeBlock`/`unschedule` in the core. M5b (Google) is next.
- **Released**: pre-release **v0.5.4** on GitHub (`tick-alt-0.5.4-arm64.apk`: the sort button stays right and names the current sort; v0.5.3 one sort for every view; v0.5.2 list/tag calendar panel sources; v0.5.1 `tt filter icon`; v0.5.0 sorting views and manual order; v0.4.1 Show in Calendar only on dated tasks; v0.4.0 arranging tags, resizable detail, Parent picker, Show in Calendar; v0.3.1 filter quick add, and Calendar listed right under Inbox in the sidebar via `ShellPage.top`; v0.3.0 the calendar, v0.2.0 before it), signed with the Arch laptop's **debug** key, so each updates the last in place. Its CI run fails until release-key secrets exist; that's expected.

**Running on the Mac now (handoff 2026-11-25, end of M2 slice 5; everything is pushed):**
- Toolchain: Xcode 27.2 (licence accepted, `xcode-select` points at it, first launch done), Rust 1.100, Flutter 3.50.1, CocoaPods 1.17.0 (Homebrew). `source scripts/env.sh` as always.
- `/Applications/Tick Alt.app` is installed: a copy of `scripts/build-macos.sh`'s build from `401962f`, with the daemon bundled. `dist/tick-alt-0.3.0-macos.dmg` is the same build.
- **The LaunchAgent is installed** (`~/Library/LaunchAgents/dev.tickalt.daemon.plist`): `tt-daemon` runs from the app's `Contents/Helpers/Tick Alt Daemon.app`, starts at login and restarts after a crash (tested with `kill -9`). Logs: `~/Library/Logs/tick-alt/daemon.log`. **After rebuilding:** copy the new app into `/Applications` (`ditto`), then `scripts/install-launch-agent.sh "/Applications/Tick Alt.app"` to restart the daemon.
- Database: `~/Library/Application Support/tick-alt/tt.db`, sync node `58385947…`. At the Mac's handoff it wasn't paired; since then the Arch laptop lists it as a peer (synced 15:56).
- The user allowed the daemon's notifications. Reminders show as banners; Done from Notification Centre completed a task. The menu-bar icon looks right (the user checked).
- **Not tested on the Mac:** Snooze, the menu's Open and Sync now, adding a task by typing in the app (background computer use can't type into Flutter), pairing.

**Next on the Mac** (ask the user which):
1. The Mac is paired with the Arch laptop now; check M3's exit: "Mac ↔ Arch stay in sync" (edit on each side, see it arrive).
2. Test Snooze, Open and Sync now from the menu bar, and typing in the app.
3. ~~On Arch: `cargo clippy -p tt-daemon`~~ ✅ done 2026-11-25 afternoon: clean, and the daemon tests pass on Linux. It hasn't been *run* on Arch yet (see "Latest session").
4. Smaller: the bundle's copyright line waits for the LICENSE name; the macOS bundle is universal (arm64 + x86_64, 120 MB unpacked) and could be arm64 only; the "Next" list below still applies.

**Running on the Arch laptop (as of its handoff):**
- The `tick-alt` user service runs this checkout's `target/release/tt-daemon` on `~/.local/share/tick-alt/tt.db` (schema v6, sync node `c382b28c2f…`, UDP 41824). **After rebuilding the daemon: `systemctl --user restart tick-alt`.**
- Desktop app and daemon: see "Latest session" above. The desktop entry is installed (rofi lists "Tick Alt").
- **The user's phone** (Xiaomi 22101316G) runs v0.2.0 and is **paired with this daemon** (2026-11-25 09:09); tasks sync both ways. A reminder round-trip (09:14 "Reminder test from the laptop": Done on the phone → completed on the laptop) was in progress when this was written; check `tt completed`.
- Toolchain: Rust 1.100, Flutter 3.50.1 (`~/flutter`), clang, `flutter_rust_bridge_codegen` 2.13.0 (+cargo-expand), Android Studio 2026.2.2 (JBR = JDK 25), SDK in `~/Android/Sdk` (platform 36, build-tools 37.0.0, NDK 28.2.13676358), emulator `tt_pixel` (headless: `emulator -avd tt_pixel -no-window -gpu swiftshader_indirect`).

**Next** (ask the user which; anything in `plan/11` is theirs to decide):
0. **Fix the `network_sim` failure** (plan/16 "Next"). The filter quick add APK is out as v0.3.1.
1. **Finish the phone checks**: did the reminder's Done reach the laptop? Does scanning the pairing QR work on the real phone camera? If not, make tickets compact (e.g. `tt3` + base64url with short field names; keep decoding `tt2`), with tests.
2. **Visual checks on desktop** (only integration tests have run these): quick add in a filter (e.g. Today → due today, Overdue → yesterday), the Devices page, the tray tooltip and Sync now, the filter builder, the right-click edit dialogs, and the start, repeat and reminder fields.
3. **Needs the user**: a release key plus the 4 CI secrets (`plan/07` "Release signing"; switching the phone to it costs one uninstall), a LICENSE file (copyright name), installing the PKGBUILD (sudo), and whether to keep the Mac's debug keystore at all (the phone now uses this laptop's).
4. **Then pick a milestone**: widget polish (choose a list or filter, scrolling, **+** quick add, midnight refresh), **M5** (calendar plus Google: Week/Day/Month, drag to schedule, OAuth, events in and tasks out), **M6** (TickTick CSV import, then switch over).
5. Smaller open items: convergent resolution of concurrent subtask cycles (`plan/14`), mobile-data sync on the real phone, the Android relay flakiness from Spike 2 (not seen on the emulator), Flutter's "migrate to built-in Kotlin" warning.

Repo: **public** at https://github.com/Belkata/utre (`origin`, everything pushed). Never commit secrets, tokens, `.key`/`.jks` files, `key.properties` or personal data.

## Setting up another machine
Per-machine state doesn't travel: Claude's memory, the repo-local git identity, keystores, the toolchain.
- Clone, then `git config user.email belkata.okfo@gmail.com` and `git config user.name "Boris Belev"` in the repo (the global email may be a work one). Push as the `Belkata` gh account. Ask before installing anything.
- Toolchain as listed above, then `source scripts/env.sh` (handles macOS and Linux paths).
- A phone accepts updates only when they're signed with the same key as the installed app. v0.2.0 is signed with **this laptop's** debug key (`~/.android/debug.keystore`). Building updates elsewhere needs that file, or better the release key (`plan/07`). **Never commit keystores.**

## Repo layout

```
core/                 utre-core: domain + SQLite. Every write is an op (oplog.rs, hlc.rs) applied with per-field LWW.
                      store (migrations/ v1–v5), task, organize, checklist, reminder, recur, filter, dates,
                      replica (version vectors, peers, trust, compaction). tests/: convergence + network_sim (proptest)
api/                  utre-api: views (sidebar/rows/detail), request protocol (proto.rs table → Request + handle + Api),
                      socket server (feature server; SyncControl hook) and client (reconnect/fallback)
sync/                 utre-sync: iroh node. replicate.rs = sync protocol 2 (tt2 pairing, trust, op streaming);
                      service.rs = SyncService (rounds, status, auto-sync) shared by daemon and phone
daemon/               utre-daemon: owns the desktop DB, serves the socket, runs SyncService, reminders. Tray: tray_sni.rs (Linux, ksni),
                      tray_mac.rs (menu bar, tray-icon), tray_none.rs. utre.service (systemd), dev.utre.daemon.plist (LaunchAgent)
cli/                  `utre` CLI (`utre --help`, `utre sync …`). Short ids = LAST 8 hex chars of the UUIDv7. $UTRE_DB, $UTRE_TZ, $UTRE_SOCKET
app/                  Flutter app (android, linux, macos)
  lib/main.dart       startup: desktop → TaskDb.connectOrOpen (daemon); Android → TaskDb.open + startPhoneSync
  lib/desk/           the shell: theme, controller, sidebar, task_list, detail, filter_builder(+filter_rule), style_dialog, devices
  lib/widget_bridge.dart   Android channel: widget refresh, reminders, zone, device name, share target
  rust/               utre_bridge: Dart API (src/api/, frb), Kotlin API (src/kotlin.rs, UniFFI), phone.rs (in-process sync), changes.rs
  linux/packaging/    dev.utre.utre.desktop + .svg icon (named after the Wayland app id)
  android/app/src/main/kotlin/dev/utre/
    core/utre_bridge.kt          GENERATED UniFFI bindings (scripts/gen-kotlin-bindings.sh)
    utre/                  MainActivity (channels, share target), widget/ (Glance), reminders/ (alarms, Done/Snooze),
                               sync/SyncWorker (WorkManager)
  test/               calendar + filter-rule tests;  integration_test/  shell tests (run with -d linux)
packaging/arch/       PKGBUILD (+ .SRCINFO)
.github/workflows/    android-release.yml (APK on v* tags; needs release-key secrets)
tools/uniffi-bindgen/ uniffi-bindgen pinned to the library's UniFFI version
scripts/              env.sh, gen-kotlin-bindings.sh, install-user-service.sh, install-desktop-entry.sh,
                      build-macos.sh (app + DMG), install-launch-agent.sh
dist/                 built APKs and DMGs for the user (git-ignored)
docs/                 the utre.party GitHub Pages site (CNAME, index.html, privacy/)
.claude/              settings.json (the bash-guard hook), multi-agent.md (the playbook, imported below)
plan/                 Obsidian plan notes + attachments/
```

There is one native library, `libutre_bridge.so`, used by Flutter, the widget and the background job. On Android they all open `Context.filesDir/utre.db` (WAL handles the writers) and share one process-wide sync node. On desktop everything goes through `utre-daemon` when it runs.

## Build and test

Always start with `source scripts/env.sh`. It sets up rustup, which is keg-only in Homebrew, plus `JAVA_HOME` (Android Studio's JBR) and `ANDROID_HOME`.

| What | Command |
|---|---|
| Rust tests + lint | `cargo test --workspace` · `cargo clippy --workspace --all-targets` |
| Dart analysis + calendar tests | `cd app && flutter analyze && flutter test` |
| Regenerate Dart bindings (after editing `app/rust/src/api/`) | `cd app && flutter_rust_bridge_codegen generate` |
| Regenerate Kotlin bindings (after editing `app/rust/src/kotlin.rs`) | `scripts/gen-kotlin-bindings.sh` |
| Android emulator | `emulator -avd tt_pixel -no-snapshot-save -no-boot-anim` (run it in the background) |
| On-device Flutter test | `cd app && flutter test integration_test/inbox_test.dart -d emulator-5554` |
| On-device Kotlin tests | `cd app/android && ./gradlew app:connectedDebugAndroidTest -Ptarget-platform=android-arm64` |
| Release APK (what to try on a device) | `timeout 30m scripts/build-apk.sh [--allow-debug]`: builds the arm64 release APK into `dist/utre-<version>-arm64.apk`, prints its signing cert SHA-256 and SHA-1, and fails if the cert differs from `packaging/android/release-cert.sha256` (or, while that file is empty, unless `--allow-debug`). Plain build: `cd app && flutter build apk --release --target-platform android-arm64` |
| macOS app + DMG (daemon bundled) | `scripts/build-macos.sh` (`--no-dmg` for the .app only) → `dist/utre-<version>-macos.dmg` |
| macOS LaunchAgent | `scripts/install-launch-agent.sh ["/Applications/Utre.app"]` (`--remove` to undo) |
| Publish a test APK | bump `version:` in `app/pubspec.yaml`, build the release APK, then `gh release create vX.Y.Z <apk> --prerelease` (see v0.1.0) |
| macOS release build (CI) | `.github/workflows/macos-release.yml` runs on every `v*` tag (or `gh workflow run macos-release.yml -f tag=vX.Y.Z`): builds `Utre.app`, uploads `utre-X.Y.Z-macos.zip` to the release |
| Sign the macOS zip | `scripts/macos-release-sign.sh X.Y.Z` (after the workflow): downloads the zip, makes `utre-X.Y.Z-macos.zip.sig` with the repo key, uploads it. The Mac's daemon updater checks it |
| Update this laptop | `sudo pacman -Syu` (`utre-bin` from our signed repo, run by the user), then `systemctl --user restart utre.service` and reopen the app. `scripts/update-desktop.sh` is only for machines running the checkout build: it rebuilds what changed (daemon and `utre` with `--profile fast`, the app as a Flutter release build), backs up the DB, restarts the daemon and relaunches the app (`--all` rebuilds everything, `--no-restart` only builds, `--check` prints `packaged` or `checkout`); on a packaged install it refuses to restart anything |
| Linux tarball for the AUR (`utre-bin`) | `scripts/build-linux-tarball.sh [--version X.Y.Z]` → `dist/utre-<version>-linux-x86_64.tar.gz`; attach it to the GitHub release |
| Publish to our pacman repo (`arch-repo` release) | `scripts/pacman-repo-publish.sh X.Y.Z [--dry-run]` (after the tarball is uploaded; signs with the repo key; `--unsigned` opts out) |
| Publish `utre-bin` to the AUR | `scripts/aur-publish.sh X.Y.Z [--dry-run]` (needs the release asset and the user's AUR account and SSH key; not available yet) |

The first Gradle build takes about 6 minutes, because cargokit compiles Rust for each Android ABI. Long builds are best run in the background.

**Build speed (Arch laptop).** `[profile.dev]` keeps line tables only, and `[profile.fast]` (release with `opt-level = 2` and incremental) is the daily driver for `utre-daemon` and `utre`; `release` is untouched (Flutter's release builds and the APK use it). mold and sccache are set in `~/.cargo/config.toml`, which is **not** in the repo: CI and the Mac don't have them, and a repo-wide linker setting would break CI's host builds. After a fresh `pacman -S mold sccache`, the first build is cold. Check the cache with `sccache --show-stats`. For quick loops use `cargo test -p <crate>` or `cargo check -p <crate>`, and `flutter run -d linux` against a test daemon for UI work; run the whole workspace only before committing.

## Gotchas (each one cost time; details in `plan/13 M0 Spike Log.md`)

- **Use `package:flutter/material.dart`, not `material_ui`.** Flutter 3.50 deprecates the former, but `material_ui` is a separate copy that kalender can't see. Import flutter/material with `// ignore: deprecated_member_use`.
- **Test Android on release builds.** R8 full mode (the AGP 9 default) strips constructors that WorkManager, Room, Glance and JNA load by reflection. The keep rules are in `app/android/app/proguard-rules.pro`; add more there when something new is loaded reflectively. Debug builds hide these bugs, and a debug APK started without `flutter run` sits on the splash screen.
- **Compose compiler for Glance.** `app/android/app/build.gradle.kts` must apply `org.jetbrains.kotlin.android` and then `org.jetbrains.kotlin.plugin.compose`, both before the Flutter plugin, because the template sets `android.builtInKotlin=false`. Verify with `./gradlew :app:dependencies --configuration kotlinCompilerPluginClasspathRelease`. Don't re-add `buildFeatures.compose`.
- **Glance calls `provideGlance` once per session.** Load data inside the composition, keyed on the `version` in the widget's Glance state. Always refresh with `TaskListWidget.refreshAll()`.
- **kalender:**
  - Dropping from outside the calendar uses its internal `Reschedule` type (a `src/` import).
  - Outside draggables need `pointerDragAnchorStrategy`.
  - The app must call `selectEvent` on tap.
  - After changing the view, jump back to today.
- **Emulator quirks:**
  - The Pixel launcher removes the widget on every `adb install -r`; re-pin it with the in-app button.
  - Gboard's stylus tutorial and Google sign-in screens pop up over the app. Dismiss them, skipping sign-in.
  - `adb input` can't do long-press drags; use Flutter widget tests instead.
- Flutter's `integration_test` plugin pins `androidx.test:runner` to 1.3.0. Don't declare a newer one.
- **Core writes go through `Store::write` and its `Writer::set` op path.** Never `UPDATE` tables directly, or sync loses the change. Don't call a public `Store` method from inside `write` (the nested `BEGIN` fails); use the `pub(crate)` helpers that take a `Writer`.
- New syncable fields need an entry in `oplog::field_spec`; anything not listed is rejected.
- `cargo fmt --all` reformats the generated `app/rust/src/frb_generated.rs`. Revert it with `git checkout` and don't commit it.
- **Release APKs are signed with the Arch laptop's debug key until the release key exists** (the template's `signingConfig`; v0.1.0 used the Mac's). A phone only accepts updates signed with the same key. Once the user writes `app/android/key.properties` (git-ignored; `build.gradle.kts` then signs with it) and puts the key's SHA-256 in `packaging/android/release-cert.sha256`, releases switch to the release key and `scripts/build-apk.sh` refuses any APK signed with another key. The phone needs one uninstall and reinstall at the switch (the in-app updater shows that as "wrong_key"), and the Google Android OAuth client needs the new SHA-1.
- **Release builds need `INTERNET` in `src/main/AndroidManifest.xml`.** The template only puts it in debug and profile.
- The Android QA plugin's Python scripts need Python ≥ 3.10; `scripts/env.sh` puts Homebrew's `python3` first.
- **macOS daemon:** AppKit owns the main thread (`tray_mac::run_app`), so tokio runs on a second thread and the menu-bar icon is updated through GCD's main queue. Don't put `#[tokio::main]` back on macOS.
- **macOS notifications need the bundled daemon** (`Utre Daemon.app` from `scripts/build-macos.sh`); bare `target/release/utre-daemon` only logs reminders. Use mac-usernotifications' **async** API: the blocking calls (and notify-rust's wrapper) fail with `MainThreadNotRunning` whenever the run loop is busy.
- **Unix socket paths max out at 104 bytes on macOS.** Keep test `$UTRE_SOCKET`s short (the scratchpad path is too long).
- After editing a podspec, run `pod install` in `app/macos`; `flutter build macos` doesn't notice. CocoaPods needs a UTF-8 locale (`scripts/env.sh` sets it).
- The macOS app has **no App Sandbox**: in it the app couldn't reach the daemon's socket or the shared DB.
- **New core calls go in the `api!` table in `api/src/proto.rs`.** That makes them reachable over the socket and as an `Api` method. Bridge view types are flutter_rust_bridge mirrors of `utre-api` types: change both together, then regenerate.
- **Android apps can't make hard links** (SELinux `avc: denied { link }` for untrusted_app): don't use `std::fs::hard_link` in code that runs on the phone (sync/src/lib.rs `publish_chain` has the fallback).
- **Never let a Rust backtrace be captured in the app process.** It segfaults: libflutter_linux_gtk.so bundles libunwind and clashes with libgcc_s. `init_app` sets `RUST_BACKTRACE=0` for that reason, so don't remove it, and don't run the app with `RUST_BACKTRACE=1`.
- **The Linux app prefers XWayland** (`app/linux/runner/main.cc`): Flutter's GTK3 embedder makes typing laggy on native Wayland and it is smooth under X11. `GDK_BACKEND` set explicitly wins; `UTRE_WAYLAND=1` forces native Wayland. The X11 WM_CLASS is lowercase `dev.utre.utre` (`gdk_set_program_class`).
- **The app's process name is `utre` for both the checkout bundle and the packaged `/usr/lib/utre/utre`**, so never kill by name (match `/proc/<pid>/exe`, or `kill <pid>`). After `pacman -Syu` the packaged daemon and app keep running the old version until `systemctl --user restart utre.service` and reopening the app.
- **The release app is single instance** (GApplication). Launching it again only raises the open copy, so a test launch of a release build while the user's copy is open doesn't start anything. Debug builds aren't unique. Test the daemon under systemd with a temporary unit (`systemd-run --user --unit=utre-test -p Restart=on-failure …`) rather than installing one.
- Hyprland 0.57 takes Lua in `hyprctl dispatch`. Useful for QA: `hl.dsp.send_shortcut({ mods = "", key = "Down", window = "class:dev.utre.utre" })` and `hl.dsp.window.close({ window = "class:dev.utre.utre" })`.
- **For QA, target windows by `address:`, never `class:dev.utre.utre`**: the class also matches the user's own running app.
- **Golden tests are Linux-only** (`app/test/golden/`); update them as described in `app/test/golden/README.md`.
- **After compaction a version vector means "has every winner up to here", not "holds every op".** Don't build logic on the latter (see plan/16 slice 3).
- Filter definitions cross the bridge as JSON strings (`app/lib/desk/filter_rule.dart` mirrors `utre_core::FilterRule`'s JSON). Keep the two in step when the core gains a field or operator.
- **Android SDK tooling:** `sdkmanager` is deprecated; use `android --no-metrics --sdk=$HOME/Android/Sdk sdk install <pkg/with/slashes>`. Flutter's accessibility tree on Android is coarse (one merged node), so drive the emulator by coordinates from `adb exec-out screencap` (1080×2400).
- The integration-test binding doesn't paint into the desktop window, so screenshots of test runs show "Test starting…". Check visuals in the real app.
- flutter_rust_bridge scans every type under `app/rust/src/api/`, private ones too. Keep helpers outside it (see `src/changes.rs`).

## Working rules (from the user)

- **Small commits.** One logical change per commit; never commit large files or big mixed changes at once. Shrink screenshots before committing them (JPEG, about 400 px wide, into `plan/attachments/`).
- No `Co-Authored-By` trailers or "Generated with" footers in commit messages.
- Work is orchestrated as described in [.claude/multi-agent.md](.claude/multi-agent.md) (imported in "Multi-agent workflow").
- Commit as `belkata.okfo@gmail.com` (this repo's `user.email`; the global git email is a work address). Push with the `Belkata` gh account.
- Check a library's actual version (lockfile, crates.io, pub.dev, Google Maven) before using its API, and record new versions in `plan/02`. Use the `context7` MCP for docs.
- Write readable multi-line shell commands with `\` continuations. Use `git -C <path>` outside the repo root.
- Plan before implementing. Interview the user when unsure; anything listed in `plan/11` is the user's call.
- Downloads, installs, licence acceptances and anything outward-facing need the user's OK first.
- Keep `plan/` updated as decisions are made: tick the roadmap and log spike results.
- **After every feature request or batch of them (once it's finished, not after each step), without asking:** commit, push, rebuild and restart the Arch daemon and the release desktop app, bump `version:` in `app/pubspec.yaml`, build the release APK, tag it and upload it as a GitHub pre-release (the "Publish a test APK" row in Build and test). Said by the user on 2026-11-25.

## Multi-agent workflow (decided 2026-11-26)

Every session uses it: the session the user talks to is the orchestrator and integrator; it plans, splits the work by area and briefs **Sonnet 5.5 subagents** (`model: "sonnet"`, in the background), each in its own worktree and PR with green CI before merge. It verifies their reports, answers the user and runs the release step. The full playbook (roles, brief template, release step, long-running command rules, lessons) is [.claude/multi-agent.md](.claude/multi-agent.md), imported here:

@.claude/multi-agent.md

## Plugins to use (installed on the account 2026-11-24, not loaded in the session that wrote this)

- **Dart and Flutter** (official): Dart MCP server for analysis, tests and running the app; also Flutter skills such as `flutter-add-widget-test` and `flutter-add-integration-test`.
- **kotlin-lsp**: Kotlin diagnostics, so widget and Gradle Kotlin errors show up before a 2-minute Gradle build.
- **Android Emulator QA**: launch the app, inspect the UI, take screenshots and collect logs on the emulator, instead of hand-written `adb` scripts.
