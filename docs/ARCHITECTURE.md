# BeaconSync architecture

BeaconSync is a git-like modpack manager. One Rust core powers three programs:

| Part | Path | What it is |
| --- | --- | --- |
| `beacon-core` | `crates/core` | Pack model, versioning engine, Modrinth/CurseForge, importers, server pack builder, hub client |
| `beacon` | `crates/cli` | CLI for servers (install, run, auto-update) and for pack authors (status, commit, push, deploy) |
| `beacon-hub` | `crates/hub` | Self-hosted sync server: accounts, pack storage, releases, deployments, server agents |
| Desktop | `apps/desktop` | Tauri 2 + React app: library, mod management, history, publish and deploy |
| Companion mod | `mods/companion-fabric` | Fabric client mod: in-game update notifications and hot pull |

## Instances

An instance is a Prism Launcher compatible folder, so Prism can launch it directly:

```
<instance>/
  instance.cfg        Prism metadata (name, icon)
  mmc-pack.json       Minecraft + loader components
  minecraft/          game dir (mods/, config/, resourcepacks/, ...)
  .beacon/            BeaconSync repo (ignored by Prism)
```

When Prism is installed, BeaconSync creates instances inside Prism's `instances/` folder and
launches them with it. Otherwise Play downloads a pinned portable Prism (checksummed) into
`<data>/beaconsync/prism/<version>/`, runs it with `--dir <data>/beaconsync/prism/data` and points
its `InstanceDir` at BeaconSync's instances folder (`crates/core/src/prism/`).

Players never see the built-in Prism's window:

- `--launch` starts the game without it; Prism quits when the game does.
- Adding an account runs a throwaway Prism on an empty data folder, which opens straight on the
  Microsoft sign-in. Once it saves the account, BeaconSync closes it and copies the entry into
  the real `accounts.json` (owner-only, like Prism writes it). Removing edits that file. Both
  wait until no Prism started by the app is running, since Prism rewrites its files, and Play
  waits for a sign-in to finish.
- Play passes the account picked in BeaconSync with `--profile`.
- Its console stays closed when things go wrong (`ShowConsoleOnError=false`); BeaconSync waits
  for Prism to exit and reports a crash report the game wrote meanwhile, or, when Prism exits
  with an error, the game's or Prism's log.
- Memory is set per instance in `instance.cfg` (`OverrideMemory`, `MaxMemAlloc`).

## Repo layout (`<instance>/.beacon/`)

```
config.json     pack settings (see below)
HEAD            "ref: refs/heads/<branch>" or a bare commit hash (detached)
refs/heads/*    branch tips, one commit hash per file
objects/ab/cd…  content addressed objects (path = first 2 hex chars / remaining chars)
index.json      stat cache only: { "<path>": { "size", "mtime", "hash" } } (safe to delete)
staged.json     metadata for files added through the app: { "<hash>": Entry }
MERGE_HEAD      present while a merge waits to be saved
```

Object id = lowercase hex SHA-512 of the exact object bytes. Objects are blobs (file contents),
trees and commits. Trees and commits are compact JSON with a deterministic key order
(readers must not depend on it; always hash the bytes you received).

### Tree

```json
{
  "format": 1,
  "minecraft": "1.21.1",
  "loader": { "kind": "fabric", "version": "0.16.9" },
  "files": {
    "config/sodium-options.json": { "hash": "<sha512>", "size": 812 },
    "mods/sodium-fabric-0.6.5.jar": {
      "hash": "<sha512>",
      "size": 1048576,
      "side": "both",
      "name": "Sodium",
      "version": "0.6.5",
      "source": { "type": "modrinth", "project": "AANobbMI", "version": "abc123", "url": "https://cdn.modrinth.com/..." }
    }
  }
}
```

- Paths are relative to `minecraft/`, always `/` separated.
- `loader.kind`: `vanilla | fabric | quilt | forge | neoforge` (`version` omitted for vanilla).
- `side`: `both | client | server`, omitted means `both`.
- `source` types: `modrinth {project, version, url}`, `curseforge {project, file, url}`, `url {url}`.
  No `source` means the bytes live in the object store (blob with the same hash).
- A disabled mod keeps its entry with a `.disabled` suffix on the path (Prism convention).
- Identity used for diff/merge: `source.project` when present, else the path without `.disabled`.

### Commit

```json
{ "format": 1, "tree": "<hash>", "parents": ["<hash>"], "author": "doc", "message": "Add Sodium", "time": 1730000000 }
```

### config.json

```json
{
  "format": 1,
  "name": "My Pack",
  "role": "local | owner | subscriber",
  "remote": { "hub": "https://hub.example.com", "pack": "<pack id>", "code": "7F3K2Q", "branch": "main" },
  "upstream": { "kind": "beacon", "hub": "...", "code": "...", "name": "...", "commit": "<commit>", "base_tree": "<tree>" }
}
```

`remote` is absent for local only packs. `upstream` is present for forks and imported modpacks
(`kind: "modrinth"` stores `project`, `name`, `version` and `base_tree`). `base_tree` is the
upstream tree at the last sync and is the merge base for "Get updates from upstream".

### Tracked files

Everything under `minecraft/` except a default ignore list (saves, logs, crash reports, screenshots,
caches, options.txt, server lists...) plus gitignore style rules in `minecraft/.beaconignore`.

## Hub API

JSON over HTTP. Auth: `Authorization: Bearer <token>`. User tokens come from login or device flow,
server tokens are scoped to a single server and only work on `/api/agent/*`.

Public (no auth), used by the companion mod and by players joining with a code:

```
GET /api/public/packs/:code
  -> { id, code, name, owner, description,
       latest: { id, name, commit, notes, created } | null,
       servers: [ { name, address, commit, online, updating, target } ] }
GET /api/objects/:hash   -> raw object bytes (blobs, trees, commits)
```

Authenticated:

```
POST /api/auth/register|login      { username, password } -> { token, user }
POST /api/auth/device/start        -> { device_code, user_code, expires_in }
POST /api/auth/device/poll         { device_code } -> { token, user } | 428 pending
POST /api/auth/device/approve      { user_code }
GET  /api/me
GET  /api/library                  packs you own or joined
POST /api/library/join             { code }
POST /api/packs                    { name, description, minecraft, loader } -> pack
GET|PATCH|DELETE /api/packs/:id
POST /api/objects/missing          { hashes } -> { missing }
PUT  /api/objects/:hash            body = bytes (hash verified)
GET  /api/packs/:id/refs           -> { "main": "<commit>" }
PUT  /api/packs/:id/refs/:branch   { old, new, force }
GET|POST /api/packs/:id/releases   { commit, name, notes }
GET|POST /api/packs/:id/servers    { name, mode, address } -> { server, token }
DELETE   /api/packs/:id/servers/:sid
POST /api/packs/:id/deploy         { release, servers: [ids] | "all" }
GET  /api/servers/:sid/console?after=N&wait=20
                                   -> { lines, next, backups, online, running }   (long poll; no `after` = last 500 lines)
POST /api/servers/:sid/actions     { kind: "command", arg } | { kind: "start|stop|restart|backup" }
                                   | { kind: "restore|delete_backup", arg: backup id }
                                   | { kind: "update", arg: BeaconSync version }   (refused for agents that don't report a version)
```

Server agent (server token):

```
GET  /api/agent/poll?wait=25&known=<deployment>
                               -> { target: { commit, release, deployment } | null, actions: [...] }   (long poll)
POST /api/agent/report         { deployment, status: "applying|done|failed|running|stopped|rolledback", message, commit, agent }
POST /api/agent/console        { lines, backups?, running? }
```

`agent` is the agent's `beacon` version; servers list it as `agent_version`.

The hub keeps the last 2000 console lines and the backup list per server in memory only; the
agent sends its backups and state again when it starts.

## Deploy flow

1. Author edits locally, the app shows the changes, author saves a version (commit).
2. Publish pushes objects + branch ref to the hub and creates a release.
3. Deploy sets the target release on the chosen servers (one, several or all).
4. Each server runs `beacon server run`, which supervises the Minecraft process and long polls
   the hub. On a new target it warns players, stops the server, backs it up, applies the
   new tree, restarts and reports back. It keeps the last 10 BeaconSync backups in
   `.beacon/backups`, and also lists world archives backup mods leave in `*backup*` folders.
5. Players with the companion mod get an in-game toast, can compare their version with the
   server's and pull configs/resource packs live (mods are staged for the next restart).

## Updates

Both the app and servers update from BeaconSync's GitHub releases (`beacon-core::release`;
`BEACON_RELEASES` points it at another releases API, e.g. a mirror or a test feed). Downloads are
checked against the size and sha256 GitHub records for each release file.

- **App.** Checks on start and every 6 hours. A button next to the logo offers the new version.
  The first click downloads the file for this kind of install (NSIS or MSI on Windows, dmg on
  macOS, AppImage, deb or rpm on Linux), the second installs it and restarts. Dev builds and
  installs it can't recognise only link to the release.
- **Servers.** The app compares each server's `agent_version` with the newest release and offers
  "Update beacon". The agent downloads and verifies the new `beacon-<target>`, runs
  `--version` on it, then warns players, stops Minecraft, swaps the binary and re-execs itself
  (same pid, so systemd and docker don't notice). If anything fails before the swap, Minecraft
  keeps running or comes back on the old `beacon`.
