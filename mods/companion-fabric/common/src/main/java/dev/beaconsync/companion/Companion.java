package dev.beaconsync.companion;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.nio.file.Path;
import java.util.Locale;
import java.util.Objects;
import java.util.Optional;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Version-independent mod logic. The per-version entrypoint forwards Fabric events here and
 * implements {@link Platform} for chat, toasts and resource reloads. All hub and disk work runs on
 * one daemon worker thread. Failures never propagate: they are logged and shown in chat.
 */
final class Companion {
    private static final Logger LOG = LoggerFactory.getLogger("BeaconSync");
    private static final long CHECK_MINUTES = 5;

    private final Platform platform;
    private final String userAgent;
    private final ScheduledExecutorService worker = Executors.newSingleThreadScheduledExecutor(task -> {
        Thread thread = new Thread(task, "BeaconSync");
        thread.setDaemon(true);
        return thread;
    });
    private final AtomicBoolean pulling = new AtomicBoolean();
    private final Optional<Repo> repo;

    // Written on the worker, read anywhere.
    private volatile Model.Pack pack;
    private volatile String localCommit;
    private volatile String serverAddress;
    private volatile Hub hub;
    // Worker thread only.
    private String notifiedCommit;
    private String lastCheckError;

    /** Finds the instance's repo and deletes jars retired by an earlier pull. Call from mod init. */
    Companion(Platform platform, Path gameDir, String userAgent) {
        this.platform = platform;
        this.userAgent = userAgent;
        Optional<Repo> found = Optional.empty();
        try {
            found = Repo.find(gameDir);
            int swept = HotPull.sweepRetired(gameDir.resolve("mods"));
            if (swept > 0) LOG.info("Removed {} retired mod jars", swept);
        } catch (Exception e) {
            LOG.warn("BeaconSync startup failed", e);
        }
        if (found.isEmpty()) LOG.info("No .beacon/config.json next to the game dir, BeaconSync is idle");
        repo = found;
    }

    /** Client started: begin periodic update checks. */
    void start() {
        if (repo.isPresent()) worker.scheduleWithFixedDelay(this::check, 0, CHECK_MINUTES, TimeUnit.MINUTES);
    }

    void stop() {
        worker.shutdownNow();
    }

    /** World joined or left. {@code address} is the server's address, null for singleplayer or on disconnect. */
    void joined(String address) {
        serverAddress = address;
        if (address != null && repo.isPresent()) worker.execute(this::checkServer);
    }

    /** Periodic: refresh hub state and announce a new release once per commit. */
    private void check() {
        try {
            refresh();
            lastCheckError = null;
            Model.Pack p = pack;
            if (p == null || p.latest() == null || p.latest().commit() == null) return;
            String latest = p.latest().commit();
            if (latest.equals(localCommit) || latest.equals(notifiedCommit)) return;
            notifiedCommit = latest;
            String text = p.name() + " " + Objects.requireNonNullElse(p.latest().name(), Hub.abbrev(latest)) + " available";
            platform.toast(text);
            platform.say(Chat.prefixed(Chat.Tone.PLAIN, text + " ").pullLink());
        } catch (Exception e) {
            String message = describe(e);
            LOG.warn("BeaconSync update check failed", e);
            // The check repeats every few minutes; only repeat the chat line when the error changes.
            if (!message.equals(lastCheckError)) platform.say(Chat.error("update check failed: " + message));
            lastCheckError = message;
        }
    }

    /** On join: tell the player whether their pack matches the server's deployed commit. */
    private void checkServer() {
        try {
            refresh();
            Model.Server server = currentServer();
            if (server == null) return;
            if (server.commit() == null) {
                platform.say(Chat.prefixed(Chat.Tone.PLAIN, server.name() + " has no BeaconSync version deployed yet"));
            } else if (server.commit().equals(localCommit)) {
                platform.say(Chat.prefixed(Chat.Tone.OK, "your pack matches " + server.name()));
            } else {
                platform.say(Chat.prefixed(Chat.Tone.WARN, server.name() + " runs " + Hub.abbrev(server.commit())
                        + ", you have " + Hub.abbrev(localCommit) + ". You need to update ").pullLink());
            }
        } catch (Exception e) {
            fail("server version check failed", e);
        }
    }

    /** {@code /beacon status}. Returns the brigadier result. */
    int status() {
        if (repo.isEmpty()) {
            platform.say(Chat.error("this instance is not a BeaconSync pack"));
            return 0;
        }
        worker.execute(() -> {
            try {
                refresh();
                Model.Pack p = pack;
                if (p == null) {
                    platform.say(Chat.prefixed(Chat.Tone.PLAIN, "local pack, no hub remote. Version " + Hub.abbrev(localCommit)));
                    return;
                }
                Model.Server server = currentServer();
                platform.say(Chat.prefixed(Chat.Tone.BOLD, p.name()));
                platform.say(Chat.row("Local", Hub.abbrev(localCommit)));
                platform.say(Chat.row("Latest", p.latest() == null ? "none"
                        : p.latest().name() + " " + Hub.abbrev(p.latest().commit()) + mark(p.latest().commit())));
                platform.say(Chat.row("Server", server == null ? "not on a BeaconSync server"
                        : server.name() + " " + Hub.abbrev(server.commit()) + mark(server.commit())));
            } catch (Exception e) {
                fail("status failed", e);
            }
        });
        return 1;
    }

    /** {@code /beacon pull}: the joined server's commit when on a known server, otherwise the latest release. */
    int pull() {
        if (repo.isEmpty()) {
            platform.say(Chat.error("this instance is not a BeaconSync pack"));
            return 0;
        }
        if (!pulling.compareAndSet(false, true)) {
            platform.say(Chat.error("a pull is already running"));
            return 0;
        }
        worker.execute(() -> {
            try {
                refresh();
                Model.Pack p = pack;
                if (p == null) {
                    platform.say(Chat.error("no hub remote configured"));
                    return;
                }
                Model.Server server = currentServer();
                String target = server != null && server.commit() != null ? server.commit()
                        : p.latest() != null ? p.latest().commit() : null;
                if (target == null) {
                    platform.say(Chat.prefixed(Chat.Tone.PLAIN, "nothing to pull, no release published yet"));
                    return;
                }
                if (target.equals(localCommit)) {
                    platform.say(Chat.prefixed(Chat.Tone.PLAIN, "already up to date (" + Hub.abbrev(target) + ")"));
                    return;
                }
                platform.say(Chat.prefixed(Chat.Tone.PLAIN, "pulling " + Hub.abbrev(target) + "..."));
                report(target, HotPull.run(repo.get(), hub, target));
            } catch (Exception e) {
                fail("pull failed", e);
            } finally {
                pulling.set(false);
            }
        });
        return 1;
    }

    private void report(String target, HotPull.Result result) {
        result.failures().forEach(f -> LOG.warn("BeaconSync pull: {}", f));
        if (result.failures().isEmpty()) {
            localCommit = target;
            platform.say(Chat.prefixed(Chat.Tone.OK, "updated to " + Hub.abbrev(target) + ": " + result.written()
                    + " files written, " + result.removed() + " removed"));
        } else {
            platform.say(Chat.error(result.failures().size() + " files failed (see log), version not changed: "
                    + result.failures().get(0)));
        }
        if (result.modsChanged() > 0) {
            platform.say(Chat.prefixed(Chat.Tone.WARN, result.modsChanged() + " mods changed, restart to apply"));
        }
        if (result.resourcesChanged()) platform.reloadResources();
    }

    /** Re-reads config and HEAD and fetches the pack from the hub. Worker thread. */
    private void refresh() throws Exception {
        Repo r = repo.orElseThrow();
        localCommit = r.headCommit().orElse(null);
        Model.Remote remote = r.config().remote();
        if (remote == null || remote.hub() == null || remote.code() == null) {
            pack = null;
            return;
        }
        Hub h = new Hub(remote.hub(), userAgent);
        pack = h.pack(remote.code());
        hub = h;
    }

    private Model.Server currentServer() {
        Model.Pack p = pack;
        String address = serverAddress;
        if (p == null || p.servers() == null || address == null) return null;
        HostPort joined = HostPort.parse(address);
        if (joined == null) return null;
        return p.servers().stream().filter(s -> joined.equals(HostPort.parse(s.address()))).findFirst().orElse(null);
    }

    private String mark(String commit) {
        if (commit == null) return "";
        return commit.equals(localCommit) ? " (you match)" : " (differs)";
    }

    private void fail(String what, Exception e) {
        LOG.warn("BeaconSync {}", what, e);
        platform.say(Chat.error(what + ": " + describe(e)));
    }

    private static String describe(Exception e) {
        return e.getMessage() != null ? e.getMessage() : e.getClass().getSimpleName();
    }

    /** Server address with the default port filled in, for comparing hub addresses with the joined server. */
    private record HostPort(String host, int port) {
        static HostPort parse(String raw) {
            if (raw == null || raw.isBlank()) return null;
            String s = raw.trim().toLowerCase(Locale.ROOT);
            String host = s;
            String port = null;
            if (s.startsWith("[")) {
                int end = s.indexOf(']');
                if (end < 0) return null;
                host = s.substring(1, end);
                if (s.startsWith(":", end + 1)) port = s.substring(end + 2);
            } else if (s.indexOf(':') > 0 && s.indexOf(':') == s.lastIndexOf(':')) {
                host = s.substring(0, s.indexOf(':'));
                port = s.substring(s.indexOf(':') + 1);
            }
            if (host.endsWith(".")) host = host.substring(0, host.length() - 1);
            try {
                return new HostPort(host, port == null ? 25565 : Integer.parseInt(port));
            } catch (NumberFormatException e) {
                return null;
            }
        }
    }
}
