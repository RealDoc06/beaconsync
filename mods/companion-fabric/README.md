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

## Targets

| | 1.21.1 jar | 26.3 jar |
| --- | --- | --- |
| Minecraft | 1.21 to 1.21.1 | 26.3.x |
| Fabric Loader | 0.15+ (built against 0.16.14) | 0.19.3+ (built against 0.20.1) |
| Fabric API | 0.116.17+1.21.1 | 0.161.0+26.3 |
| Java | 21 | 25 |

Needs Fabric API's command-api-v2, lifecycle-events-v1 and networking-api-v1 modules.

## Layout

- `common/`: everything that does not touch Minecraft (`Companion` flow, `HotPull`, `Hub`, `Repo`, `Model`, `Chat`) plus the JUnit tests. Plain Java, compiled once for release 21 and packed into both jars. `fabric.mod.json` lives here too, templated per target.
- `v1_21_1/`, `v26_3/`: one `BeaconCompanion` each, the Minecraft-facing layer (Fabric events, client commands, chat rendering, toasts, resource reload). Versions in each target's `gradle.properties`.
- `target.gradle`: shared setup applied by every target.

One Loom (1.19.3) drives both: `fabric-loom-remap` with Mojang mappings for obfuscated 1.21.1, plain `fabric-loom` for unobfuscated 26.x. Loom 1.18+ runs on JDK 25 only, so Gradle runs on 25 (`gradle/gradle-daemon-jvm.properties` picks an installed JDK 25 even when `JAVA_HOME` is older). The 1.21.1 target still compiles with `--release 21`.

Adding a version: copy a `v*` folder, bump its `gradle.properties`, fix the API spots in `BeaconCompanion`, add it to `settings.gradle`.

## Build

Needs a JDK 25 that Gradle can find (`JAVA_HOME`, a standard JDK location, or `org.gradle.java.installations.paths` in `~/.gradle/gradle.properties`).

```sh
./gradlew build      # builds both jars into build/libs/ and runs the tests
```

- `build/libs/beaconsync-companion-<version>+1.21.1.jar`
- `build/libs/beaconsync-companion-<version>+26.3.jar`

Drop the matching jar in the instance's `minecraft/mods/` together with Fabric API.
