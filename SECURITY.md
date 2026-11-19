# Security policy

## Supported versions

BeaconSync is in alpha. Only the latest `0.1.0-alpha` release gets security fixes; update to it
before reporting.

## Reporting a vulnerability

Please report privately, not in a public issue: open the repo's **Security** tab and choose
**Report a vulnerability** (GitHub private vulnerability reporting).

Include what is affected, the version, and how to reproduce it.

## What to expect

- An acknowledgement within 7 days.
- An assessment and, if confirmed, a fix plan shortly after. Serious issues are fixed in a new
  release first and disclosed in a GitHub security advisory once it is out.
- Credit in the advisory, unless you'd rather stay anonymous.

## Scope

- The `beacon` CLI and server agent (`crates/cli`, `crates/core`)
- The desktop app (`apps/desktop`)
- The companion mods (`mods/`)
- The hub server (`crates/hub`) and the public hub at `beaconsync.realdoc06.it`

Out of scope: denial of service by flooding the public hub, issues in Minecraft, mod loaders,
Prism Launcher, Modrinth or CurseForge themselves, and mods a pack owner chose to ship (a pack
runs its mods' code by design).

Please test against your own hub (`beacon-hub`), not the public one.
