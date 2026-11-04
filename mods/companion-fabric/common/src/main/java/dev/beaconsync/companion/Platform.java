package dev.beaconsync.companion;

/** The Minecraft-facing side, implemented once per target version. Every method may be called from any thread. */
interface Platform {
    /** Shows a chat line now, or on the next world join when none is open. */
    void say(Chat message);

    /** Shows a system toast titled "BeaconSync". */
    void toast(String text);

    /** Reloads resource packs on the client thread. */
    void reloadResources();

    /** Adds the server to the multiplayer list unless one with that address is there. */
    void addServer(String name, String address);

    /** Joins the server, unless the player is already in a world or busy in another screen. */
    void reconnect(String name, String address);
}
