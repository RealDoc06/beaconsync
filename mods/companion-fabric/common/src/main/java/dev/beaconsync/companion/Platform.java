package dev.beaconsync.companion;

/** The Minecraft-facing side, implemented once per target version. Every method may be called from any thread. */
interface Platform {
    /** Shows a chat line now, or on the next world join when none is open. */
    void say(Chat message);

    /** Shows a system toast titled "BeaconSync". */
    void toast(String text);

    /** Reloads resource packs on the client thread. */
    void reloadResources();
}
