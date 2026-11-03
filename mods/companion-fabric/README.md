# BeaconSync Companion (Fabric)

Client-only Fabric mod for BeaconSync instances. Idle unless `<instance>/.beacon/config.json` exists next to the game dir.

- Checks the hub (`remote.hub` + `remote.code`) on start and every 5 minutes. A new release shows a toast and a clickable chat line once.
- On joining a server listed in the pack's `servers[]` (host + port, default 25565), says whether your version matches.
- `/beacon status`: local version, latest release, joined server's version.
- `/beacon pull`: hot pull. Targets the joined server's commit when on a known server, else the latest release.
  - Configs and resource packs apply live (resources reload when `resourcepacks/` or `assets/` change).
  - Mod jars are staged: old jars become `*.jar.beacon-old` (deleted on next start), restart to apply.
  - Every file is SHA-512 verified and written atomically. HEAD (branch ref) only moves when every file applied; `.beacon/index.json` is dropped so the desktop app rehashes.

All network and disk work runs on a background thread. Failures are logged and shown in chat, never thrown.

## Versions

| | |
| --- | --- |
| Minecraft | 1.21 to 1.21.1 |
| Fabric Loader | 0.15+ (built against 0.16.14) |
| Fabric API | command-api-v2, lifecycle-events-v1, networking-api-v1 |
| Java | 21 |

Built with Fabric Loom 1.17.2 (newest Loom that runs on JDK 21) and Mojang mappings, so a 26.x port keeps the same names.

## Build

```sh
./gradlew build      # -> build/libs/beaconsync-companion-<version>.jar, runs tests
```

Drop the jar in the instance's `minecraft/mods/` together with Fabric API.
