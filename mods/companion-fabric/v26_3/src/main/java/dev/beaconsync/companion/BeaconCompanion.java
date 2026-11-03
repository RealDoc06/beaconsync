package dev.beaconsync.companion;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.command.v2.ClientCommands;
import net.fabricmc.fabric.api.client.command.v2.ClientCommandRegistrationCallback;
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
import net.minecraft.network.chat.Style;

import java.util.ArrayList;
import java.util.List;

/** Minecraft 26.3 entrypoint: wires Fabric events to {@link Companion} and renders {@link Chat}. */
public final class BeaconCompanion implements ClientModInitializer, Platform {
    // Client thread only: chat lines produced before a world was open.
    private final List<Component> pending = new ArrayList<>();

    @Override
    public void onInitializeClient() {
        FabricLoader loader = FabricLoader.getInstance();
        Companion companion = new Companion(this, loader.getGameDir(), userAgent(loader));

        ClientLifecycleEvents.CLIENT_STARTED.register(client -> companion.start());
        ClientLifecycleEvents.CLIENT_STOPPING.register(client -> companion.stop());

        ClientPlayConnectionEvents.JOIN.register((handler, sender, client) -> {
            pending.forEach(client.gui.hud.getChat()::addClientSystemMessage);
            pending.clear();
            ServerData server = client.getCurrentServer();
            companion.joined(server == null || client.isLocalServer() ? null : server.ip);
        });
        ClientPlayConnectionEvents.DISCONNECT.register((handler, client) -> companion.joined(null));

        ClientCommandRegistrationCallback.EVENT.register((dispatcher, registries) -> dispatcher.register(
                ClientCommands.literal("beacon")
                        .then(ClientCommands.literal("status").executes(ctx -> companion.status()))
                        .then(ClientCommands.literal("pull").executes(ctx -> companion.pull()))));
    }

    @Override
    public void say(Chat message) {
        Minecraft mc = Minecraft.getInstance();
        Component line = render(message);
        mc.execute(() -> {
            if (mc.player != null) mc.gui.hud.getChat().addClientSystemMessage(line);
            else pending.add(line);
        });
    }

    @Override
    public void toast(String text) {
        Minecraft mc = Minecraft.getInstance();
        mc.execute(() -> SystemToast.add(mc.gui.toastManager(), SystemToast.SystemToastId.PERIODIC_NOTIFICATION,
                Component.literal("BeaconSync"), Component.literal(text)));
    }

    @Override
    public void reloadResources() {
        Minecraft mc = Minecraft.getInstance();
        mc.execute(mc::reloadResourcePacks);
    }

    private static String userAgent(FabricLoader loader) {
        return "BeaconSync-Companion/" + loader.getModContainer("beaconsync-companion")
                .map(mod -> mod.getMetadata().getVersion().getFriendlyString())
                .orElse("dev");
    }

    /** First span is the root, the rest are appended so they inherit its style. */
    private static Component render(Chat chat) {
        List<Chat.Span> spans = chat.spans();
        MutableComponent root = literal(spans.get(0));
        spans.subList(1, spans.size()).forEach(span -> root.append(literal(span)));
        return root;
    }

    private static MutableComponent literal(Chat.Span span) {
        return Component.literal(span.text()).withStyle(style -> style(style, span.tone()));
    }

    private static Style style(Style style, Chat.Tone tone) {
        return switch (tone) {
            case PLAIN -> style;
            case PREFIX -> style.withColor(ChatFormatting.AQUA);
            case OK -> style.withColor(ChatFormatting.GREEN);
            case WARN -> style.withColor(ChatFormatting.YELLOW);
            case ERROR -> style.withColor(ChatFormatting.RED);
            case LABEL -> style.withColor(ChatFormatting.GRAY);
            case VALUE -> style.withColor(ChatFormatting.WHITE);
            case BOLD -> style.withBold(true);
            case PULL_LINK -> style.withColor(ChatFormatting.GREEN).withUnderlined(true)
                    .withClickEvent(new ClickEvent.RunCommand(Chat.PULL_COMMAND))
                    .withHoverEvent(new HoverEvent.ShowText(Component.literal("Run " + Chat.PULL_COMMAND)));
        };
    }
}
