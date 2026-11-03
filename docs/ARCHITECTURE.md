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

When Prism is installed, BeaconSync creates instances inside Prism's `instances/` folder.

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
       servers: [ { name, address, commit, online } ] }
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
```

Server agent (server token):

```
GET  /api/agent/poll?wait=25   -> { target: { commit, release, deployment } | null }   (long poll)
POST /api/agent/report         { deployment, status: "applying|done|failed", message, commit }
```

## Deploy flow

1. Author edits locally, the app shows the changes, author saves a version (commit).
2. Publish pushes objects + branch ref to the hub and creates a release.
3. Deploy sets the target release on the chosen servers (one, several or all).
4. Each server runs `beacon server run`, which supervises the Minecraft process and long polls
   the hub. On a new target it warns players, stops the server, keeps one backup, applies the
   new tree, restarts and reports back.
5. Players with the companion mod get an in-game toast, can compare their version with the
   server's and pull configs/resource packs live (mods are staged for the next restart).
