package dev.beaconsync.companion;

import com.sun.net.httpserver.HttpServer;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;
import java.util.stream.Collectors;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Runs a hot pull against an in-process fake hub serving /api/objects/:hash. */
class HotPullTest {
    private final Map<String, byte[]> hubObjects = new HashMap<>();
    private HttpServer server;
    private Hub hub;
    @TempDir Path instance;
    private Path game, beacon;

    @BeforeEach
    void setUp() throws IOException {
        server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        server.createContext("/api/objects/", ex -> {
            byte[] body = hubObjects.get(ex.getRequestURI().getPath().substring("/api/objects/".length()));
            ex.sendResponseHeaders(body == null ? 404 : 200, body == null ? -1 : body.length);
            if (body != null) ex.getResponseBody().write(body);
            ex.close();
        });
        server.start();
        hub = new Hub("http://127.0.0.1:" + server.getAddress().getPort() + "/", "BeaconSync-Test");
        game = Files.createDirectories(instance.resolve("minecraft"));
        beacon = Files.createDirectories(instance.resolve(".beacon"));
        Files.writeString(beacon.resolve("config.json"), "{}");
        Files.writeString(beacon.resolve("HEAD"), "ref: refs/heads/main\n");
        Files.writeString(beacon.resolve("index.json"), "{}");
    }

    @AfterEach
    void tearDown() {
        server.stop(0);
    }

    @Test
    void appliesTreeDiff() throws IOException {
        Repo repo = Repo.find(game).orElseThrow();
        String oldCommit = commit(Map.of("config/x.json", "old", "mods/a.jar", "AAAA", "config/gone.txt", "bye"), "");
        for (String hash : hubObjects.keySet()) repo.writeObject(hash, hubObjects.get(hash));
        repo.setHead(oldCommit);
        write("config/x.json", "old");
        write("mods/a.jar", "AAAA");
        write("config/gone.txt", "bye");
        hubObjects.remove(Repo.sha512("AAAA".getBytes(StandardCharsets.UTF_8))); // the .disabled copy must come from disk

        String target = commit(Map.of("config/x.json", "new!", "mods/a.jar.disabled", "AAAA",
                "mods/b.jar", "BBBBB", "resourcepacks/p.zip", "zip"), ",\"server/only.txt\":" + entry("srv", "server"));
        HotPull.Result result = HotPull.run(repo, hub, target);

        assertTrue(result.failures().isEmpty(), result.failures().toString());
        assertEquals(Map.of("config/x.json", "new!", "mods/a.jar.disabled", "AAAA", "mods/a.jar.beacon-old", "AAAA",
                "mods/b.jar", "BBBBB", "resourcepacks/p.zip", "zip"), gameFiles());
        assertEquals(3, result.modsChanged());
        assertTrue(result.resourcesChanged());
        assertEquals(target, Files.readString(beacon.resolve("refs/heads/main")).trim());
        assertFalse(Files.exists(beacon.resolve("index.json")));
        assertEquals(1, HotPull.sweepRetired(game.resolve("mods")));
    }

    @Test
    void rejectsPathsOutsideGameDir() throws IOException {
        Repo repo = Repo.find(game).orElseThrow();
        String target = commit(Map.of("../escape.txt", "x"), "");
        HotPull.Result result = HotPull.run(repo, hub, target);

        assertEquals(1, result.failures().size());
        assertFalse(Files.exists(instance.resolve("escape.txt")));
        assertFalse(Files.exists(beacon.resolve("refs/heads/main")), "HEAD must not move after a failure");
    }

    /** Publishes blobs, a tree and a commit to the fake hub and returns the commit hash. */
    private String commit(Map<String, String> files, String extraEntries) {
        String entries = files.entrySet().stream()
                .map(e -> "\"" + e.getKey() + "\":" + entry(e.getValue(), null))
                .collect(Collectors.joining(","));
        String tree = publish("{\"files\":{" + entries + extraEntries + "}}");
        return publish("{\"tree\":\"" + tree + "\"}");
    }

    private String entry(String content, String side) {
        String hash = publish(content);
        return "{\"hash\":\"" + hash + "\",\"size\":" + content.length() + (side == null ? "" : ",\"side\":\"" + side + "\"") + "}";
    }

    private String publish(String content) {
        byte[] bytes = content.getBytes(StandardCharsets.UTF_8);
        String hash = Repo.sha512(bytes);
        hubObjects.put(hash, bytes);
        return hash;
    }

    private void write(String path, String content) throws IOException {
        Path file = game.resolve(path);
        Files.createDirectories(file.getParent());
        Files.writeString(file, content);
    }

    private Map<String, String> gameFiles() throws IOException {
        try (var walk = Files.walk(game)) {
            return walk.filter(Files::isRegularFile).collect(Collectors.toMap(
                    p -> game.relativize(p).toString().replace('\\', '/'),
                    p -> {
                        try {
                            return Files.readString(p);
                        } catch (IOException e) {
                            throw new RuntimeException(e);
                        }
                    }));
        }
    }
}
