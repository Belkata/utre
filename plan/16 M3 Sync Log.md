---
tags: [project/tick-alt, plan, m3]
created: 2026-11-24
---

# 16 M3 Sync Log

Back to [[00 Overview]] · [[10 Roadmap]] · [[05 P2P Sync]].

## Slice 1: protocol, pairing and CLI ✅ (2026-11-24, on the Arch laptop, done while the user was away)
- **Core** (`core/src/replica.rs`, schema v3):
  - `version_vector()` (the newest HLC per device, one `GROUP BY` on the HLC's device suffix) and `ops_missing(their_vv)`.
  - A local `peer` table (node id, name, added and last seen, last address). It isn't op-logged, synced or part of `state_dump`.
- **Protocol** (`sync/src/replicate.rs`, ALPN `tickalt/sync/1`, JSON-line frames):
  - Hello (proto, name, address, vector as a list of HLCs) → ops both ways in batches of 500, sent and received at the same time → Done → Ack.
  - An untrusted Hello gets **Denied**. Pairing: a `tt2…` ticket (address, a 16-byte one-time secret, name) is valid for 5 minutes and works once. Pair → Paired, both sides trust each other, then they sync.
  - The database is opened per call (`FileReplica`), so a running daemon notices sync writes through `data_version`.
- **Node:** prefers UDP port **41824** (`$TT_SYNC_PORT`; random if taken), so LAN peers find each other again after restarts without relays. The Spike 2 ALPN is still served.
- **CLI:** `tt sync pair | join <ticket> | now | peers | forget <device> | serve`. `tt sync pull` is the Spike 2 pull.
- **Tests:** 7 in tt-sync over loopback with relays off:
  - pairing then sync converges, and later edits on both sides converge either way;
  - a quiet sync moves nothing;
  - unpaired devices are refused, and a ticket works once;
  - a 2,800-op sync streams in batches both ways.
  - Plus 2 core tests. The core's convergence property tests still pass.
- **Live CLI run** (two scratch DBs): pair and join, then `sync now` after the server restarted. A conflicting rename resolved to the later write on both sides.

## Slice 2: sync in the daemon ✅ (2026-11-25, on the Arch laptop)
- **tt-api:**
  - `SyncPair`, `SyncJoin{ticket}`, `SyncNow` and `SyncStatus` requests, answered by the daemon through `server::SyncControl` (tt-api stays free of iroh). Without one they answer "this tt-daemon doesn't run sync".
  - `peers` and `remove_peer` are in the table.
  - `Db::subscribe_writes()` fires only for client writes, so auto-sync ignores its own writes, which arrive as outside commits. An idle 25 s check showed no sync loop.
- **Daemon** (`daemon/src/sync.rs`):
  - Starts the sync node (relays unless `--no-relay`; `--no-sync` turns it off).
  - Auto-sync 5 s after startup, every 90 s, and 3 s after local edits, with no overlapping rounds.
  - Status: last round, last error, peers. Peers' incoming syncs count too.
  - Tray: the tooltip has a sync line and the menu shows the status plus a working **Sync now**.
- **CLI:** while the daemon serves this DB, `tt sync pair | join | now | peers | forget` go through it (only one process uses the key). `serve` and the spike `pull` are refused.
- **App:** desktop no longer starts the Spike 2 `SyncPeer`, which would share `tt.key` with the daemon. The sidebar's "More" section has **Devices**:
  - this device's id and its last sync;
  - Pair (shows the ticket, Copy) and Join (paste a ticket);
  - Sync now, and the paired devices with Forget.
  - Without a daemon it says how to start one. Android keeps the spike tab until M4.
- **Tests:** tt-api 5 socket tests (sync requests need a node; peers over the socket) and app integration **10/10** (Devices without a daemon).
- **Live run:** two throwaway daemons as temporary systemd units, relays off. Paired through `tt sync pair` and `join` via the daemons; a task added on one reached the other through **auto-sync alone** in under 7 s; `peers` and `sync now` worked.
- **The user's service** runs it (node `c382b28c2f…`, no peers yet). The key `~/.local/share/tick-alt/tt.key` came from the earlier spike app run.

## Slice 3: simulation, trust sharing and compaction ✅ (2026-11-25, while the user was away)
- **Network simulation** (`core/tests/network_sim.rs`, plan/05 "Testing the sync engine"): 4 replicas, random interleavings.
  - Steps: edits; one-way sends cut off mid-stream (only a prefix in HLC order arrives, like a dropped connection between batches); full sessions; retried batches; compaction at any moment. Partitions come from pairs that don't meet.
  - Checks: retried batches change nothing; compaction keeps the state and the vector; without compaction, each replica's ops from a device form an unbroken prefix of that device's log; after two rings of sessions every replica has the same state and vector.
  - 96 cases per run (about 4 s). Stress-run with 3 × 1,000 cases.
  - A mutation (delivering a stream's *end* instead of its start) is caught, so the test can fail.
  - The shared edit generators moved to `core/tests/common/`.
- **Trust sharing** (plan/05 pairing step 4, schema v4 `peer_removed`):
  - Each session now ends with a **Trust** frame (sync **protocol 2**): who the sender trusts (with when it paired them) and who it forgot.
  - `Store::merge_trust` adds unknown peers and drops forgotten ones; the newer of "paired" and "forgotten" wins, by wall clock. It never adds this device or touches the sender. `remove_peer` leaves a tombstone, and an explicit re-pair clears it.
  - Sync reports list `learned` and `dropped`; `tt` prints them, the daemon logs them and refreshes its peer count.
  - Test: B and C each pair with A only, then sync with each other directly; A forgets C, and at B's next sync with A, B forgets C and C is refused.
- **Compaction** (schema v5 `device_clock`):
  - `Store::compact()` drops ops a newer op on the same field superseded. Under per-field LWW only winners shape state, so this is safe whatever peers have seen: a peer missing a dropped op still gets the winner, which is never dropped.
  - **A compacted log is the snapshot**: a new device bootstraps from it, so no separate snapshot mechanism is needed.
  - The version vector also reads `device_clock` (each device's newest HLC, recorded before dropping), so it never goes backwards and peers don't resend compacted ops.
  - ⚠️ After compaction a vector means "has every winner up to here", no longer "holds every op". The stress run found the simulation's old retried-batch step relied on the latter; it now resends ops the receiver really holds.
  - The daemon compacts a minute after startup, then daily. `tt sync compact` does it by hand (a test task renamed 5 times dropped 5 ops).
- The user's service runs this build (real DB at schema v5, same node id, no peers). The release app was rebuilt too.

## Next
- **Check with the user:** the Devices page and the tray line in the real app, and a real second device (the Mac, or the phone after M4). `git push`: these commits are local.
- Still open in M3: QR pairing (Android, M4). Subtask cycles from concurrent moves (from plan/14's known gaps) need a convergent resolution.
- ✅ **`network_sim` fixed (2026-11-26).** Not a convergence bug: the replicas' *state* was always equal. After `compact()` a replica keeps the HLC of the ops it dropped in `device_clock` (so its vector can't go backwards), but nobody can send a dropped op, so that counter never reaches the others and the vectors differed (same wall time, counter 3 vs 6). The test now compares full vectors only when nothing compacted; with compaction it checks the same devices are covered, equal state and nothing owed (`missing` empty). The seed is saved in `core/tests/network_sim.proptest-regressions`, and CI no longer skips the test.

## Hardening review (2026-11-26, v0.8.0; no schema change, still v9)
- Synced numeric ranges are bounded in `core/src/oplog.rs` `field_spec` (instants +-2^47 ms, `duration_min` 0..=1e8, `offset_min` +-1e8). The HLC counter rolling over is handled. Ticket unhex/decode panics fixed.
- Op frames are batched to about 1 MiB; a 30 s first-frame timeout; `utre.key` is created atomically (temp file plus no-clobber hard link).
- Left as recommendations (see plan/11): reject far-future HLC wall times, validate trust device ids as 64 hex, an overall sync-session timeout.

## network_sim flake explained (2026-11-26, v0.8.1, PR 32)
- It was a **test bug**, not a convergence bug. In a ring (0-1, 1-2, 2-3, 3-0) plus compaction, one replica can be left holding a superseded op that nobody else has (the others dropped it before it reached them). Its version vector then differs, while the state is identical.
- The test now asserts identical state on every replica after the ring. Log-level checks (version vectors equal, nothing owed) run after one all-pairs pass in compacted runs. The failing case is committed as `a_superseded_op_can_be_stranded_by_compaction_in_a_ring`; 50 consecutive runs passed.
- CI prints the minimal failing input, since proptest can't persist regressions there.

## Device key on Android (2026-11-28, v0.8.13, PR 72)
- **Bug:** a fresh Android install couldn't start sync (v0.8.0 to v0.8.12). Creating the device key `utre.key` failed with "Permission denied", so the Devices page stayed at "Starting sync".
- **Cause:** SELinux forbids `link` for `untrusted_app` on `app_data_file` (`avc: denied { link }`), and the key was published with a hard link (introduced by 59139c4 on 2026-11-26).
- **Fix:** the key is published with a hard link, falling back to `renameat2(RENAME_NOREPLACE)`, then a checked rename (`publish_chain` in `sync/src/lib.rs`).
- Only fresh installs were hit; existing installs already had a key. Verified on the real phone with a test build: no denial, the sync node starts. After a reinstall the identity is new, so the phone must be paired again.
