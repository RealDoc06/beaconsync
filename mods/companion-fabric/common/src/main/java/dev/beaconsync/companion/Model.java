package dev.beaconsync.companion;

import java.util.List;
import java.util.Map;

/** Gson shapes for the slice of docs/ARCHITECTURE.md the mod reads. Unknown keys are ignored. */
final class Model {
    private Model() {}

    record Config(String name, Remote remote) {}

    record Remote(String hub, String pack, String code, String branch) {}

    /** GET /api/public/packs/:code */
    record Pack(String id, String code, String name, Release latest, List<Server> servers) {}

    record Release(String id, String name, String commit, String notes) {}

    record Server(String name, String address, String commit, Boolean online) {}

    record Commit(String tree, List<String> parents, String message) {}

    record Tree(Map<String, Entry> files) {}

    record Entry(String hash, long size, String side, Source source) {
        boolean onClient() {
            return !"server".equals(side);
        }
    }

    record Source(String type, String url) {}
}
