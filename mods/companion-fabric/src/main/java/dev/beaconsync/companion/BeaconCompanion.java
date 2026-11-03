package dev.beaconsync.companion;

import com.mojang.brigadier.context.CommandContext;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.command.v2.ClientCommandManager;
import net.fabricmc.fabric.api.client.command.v2.ClientCommandRegistrationCallback;
import net.fabricmc.fabric.api.client.command.v2.FabricClientCommandSource;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.ChatFormatting;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.toasts.SystemToast;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.network.chat.ClickEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.HoverEvent;
import net.minecraft.network.chat.MutableComponent;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Objects;
import java.util.Optional;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Client entrypoint. All hub and disk work runs on one daemon worker thread; anything that touches
 * Minecraft is handed back with {@link Minecraft#execute}. Failures never propagate: they are logged
 * and shown in chat.
 */
public final class BeaconCompanion implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("BeaconSync");

    private static final long CHECK_MINUTES = 5;
    private static final String PULL_COMMAND = "/beacon pull";

    private final ScheduledExecutorService worker = Executors.newSingleThreadScheduledExecutor(task -> {
        Thread thread = new Thread(task, "BeaconSync");
        thread.setDaemon(true);
        return thread;
    });
    private final AtomicBoolean pulling = new AtomicBoolean();

    private Optional<Repo> repo = Optional.empty();
    // Written on the worker, read anywhere.
    private volatile Model.Pack pack;
    private volatile String localCommit;
    private volatile String serverAddress;
    private volatile Hub hub;
    // Worker thread only.
    private String notifiedCommit;
    private String lastCheckError;
    // Client thread only: chat lines produced before a world was open.
    private final List<Component> pending = new ArrayList<>();

    @Override
    public void onInitializeClient() {
        try {
            repo = Repo.find(FabricLoader.getInstance().getGameDir());
            int swept = HotPull.sweepRetired(FabricLoader.getInstance().getGameDir().resolve("mods"));
            if (swept > 0) LOG.info("Removed {} retired mod jars", swept);
        } catch (Exception e) {
            LOG.warn("BeaconSync startup failed", e);
        }
        if (repo.isEmpty()) LOG.info("No .beacon/config.json next to the game dir, BeaconSync is idle");

        ClientLifecycleEvents.CLIENT_STARTED.register(client ->
                repo.ifPresent(r -> worker.scheduleWithFixedDelay(this::check, 0, CHECK_MINUTES, TimeUnit.MINUTES)));
        ClientLifecycleEvents.CLIENT_STOPPING.register(client -> worker.shutdownNow());

        ClientPlayConnectionEvents.JOIN.register((handler, sender, client) -> {
            pending.forEach(client.gui.getChat()::addMessage);
            pending.clear();
            ServerData server = client.getCurrentServer();
            serverAddress = server == null || client.isLocalServer() ? null : server.ip;
            if (serverAddress != null) worker.execute(this::checkServer);
        });
        ClientPlayConnectionEvents.DISCONNECT.register((handler, client) -> serverAddress = null);

        ClientCommandRegistrationCallback.EVENT.register((dispatcher, registries) -> dispatcher.register(
                ClientCommandManager.literal("beacon")
                        .then(ClientCommandManager.literal("status").executes(this::status))
                        .then(ClientCommandManager.literal("pull").executes(this::pull))));
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
            toast(text);
            say(prefix().append(text + " ").append(pullLink()));
        } catch (Exception e) {
            String message = describe(e);
            LOG.warn("BeaconSync update check failed", e);
            // The check repeats every few minutes; only repeat the chat line when the error changes.
            if (!message.equals(lastCheckError)) say(error("update check failed: " + message));
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
                say(prefix().append(server.name() + " has no BeaconSync version deployed yet"));
            } else if (server.commit().equals(localCommit)) {
                say(prefix().append(Component.literal("your pack matches " + server.name()).withStyle(ChatFormatting.GREEN)));
            } else {
                say(prefix().append(Component.literal(server.name() + " runs " + Hub.abbrev(server.commit())
                        + ", you have " + Hub.abbrev(localCommit) + ". You need to update ").withStyle(ChatFormatting.YELLOW))
                        .append(pullLink()));
            }
        } catch (Exception e) {
            fail("server version check failed", e);
        }
    }

    private int status(CommandContext<FabricClientCommandSource> ctx) {
        if (repo.isEmpty()) {
            say(error("this instance is not a BeaconSync pack"));
            return 0;
        }
        worker.execute(() -> {
            try {
                refresh();
                Model.Pack p = pack;
                if (p == null) {
                    say(prefix().append("local pack, no hub remote. Version " + Hub.abbrev(localCommit)));
                    return;
                }
                Model.Server server = currentServer();
                say(prefix().append(Component.literal(p.name()).withStyle(ChatFormatting.BOLD)));
                say(line("Local", Hub.abbrev(localCommit)));
                say(line("Latest", p.latest() == null ? "none"
                        : p.latest().name() + " " + Hub.abbrev(p.latest().commit()) + mark(p.latest().commit())));
                say(line("Server", server == null ? "not on a BeaconSync server"
                        : server.name() + " " + Hub.abbrev(server.commit()) + mark(server.commit())));
            } catch (Exception e) {
                fail("status failed", e);
            }
        });
        return 1;
    }

    /** Pulls the joined server's commit when on a known server, otherwise the latest release. */
    private int pull(CommandContext<FabricClientCommandSource> ctx) {
        if (repo.isEmpty()) {
            say(error("this instance is not a BeaconSync pack"));
            return 0;
        }
        if (!pulling.compareAndSet(false, true)) {
            say(error("a pull is already running"));
            return 0;
        }
        worker.execute(() -> {
            try {
                refresh();
                Model.Pack p = pack;
                if (p == null) {
                    say(error("no hub remote configured"));
                    return;
                }
                Model.Server server = currentServer();
                String target = server != null && server.commit() != null ? server.commit()
                        : p.latest() != null ? p.latest().commit() : null;
                if (target == null) {
                    say(prefix().append("nothing to pull, no release published yet"));
                    return;
                }
                if (target.equals(localCommit)) {
                    say(prefix().append("already up to date (" + Hub.abbrev(target) + ")"));
                    return;
                }
                say(prefix().append("pulling " + Hub.abbrev(target) + "..."));
                HotPull.Result result = HotPull.run(repo.get(), hub, target);
                report(target, result);
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
            say(prefix().append(Component.literal("updated to " + Hub.abbrev(target) + ": " + result.written()
                    + " files written, " + result.removed() + " removed").withStyle(ChatFormatting.GREEN)));
        } else {
            say(error(result.failures().size() + " files failed (see log), version not changed: "
                    + result.failures().get(0)));
        }
        if (result.modsChanged() > 0) {
            say(prefix().append(Component.literal(result.modsChanged() + " mods changed, restart to apply")
                    .withStyle(ChatFormatting.YELLOW)));
        }
        if (result.resourcesChanged()) Minecraft.getInstance().execute(() -> Minecraft.getInstance().reloadResourcePacks());
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
        Hub h = new Hub(remote.hub());
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
        say(error(what + ": " + describe(e)));
    }

    private static String describe(Exception e) {
        return e.getMessage() != null ? e.getMessage() : e.getClass().getSimpleName();
    }

    /** Shows a chat line now, or on the next world join when none is open. Any thread. */
    private void say(Component message) {
        Minecraft mc = Minecraft.getInstance();
        mc.execute(() -> {
            if (mc.player != null) mc.gui.getChat().addMessage(message);
            else pending.add(message);
        });
    }

    private static void toast(String text) {
        Minecraft mc = Minecraft.getInstance();
        mc.execute(() -> mc.getToasts().addToast(SystemToast.multiline(mc, SystemToast.SystemToastId.PERIODIC_NOTIFICATION,
                Component.literal("BeaconSync"), Component.literal(text))));
    }

    private static MutableComponent prefix() {
        return Component.literal("[BeaconSync] ").withStyle(ChatFormatting.AQUA);
    }

    private static Component error(String text) {
        return prefix().append(Component.literal(text).withStyle(ChatFormatting.RED));
    }

    private static Component line(String label, String value) {
        return Component.literal(label + ": ").withStyle(ChatFormatting.GRAY).append(Component.literal(value).withStyle(ChatFormatting.WHITE));
    }

    private static Component pullLink() {
        return Component.literal("[Update]").withStyle(style -> style
                .withColor(ChatFormatting.GREEN)
                .withUnderlined(true)
                .withClickEvent(new ClickEvent(ClickEvent.Action.RUN_COMMAND, PULL_COMMAND))
                .withHoverEvent(new HoverEvent(HoverEvent.Action.SHOW_TEXT, Component.literal("Run " + PULL_COMMAND))));
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
