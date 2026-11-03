package dev.beaconsync.companion;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.stream.Collectors;

/**
 * Moves the game dir from the local HEAD tree to {@code target}'s tree while the game runs.
 * Mod jars are staged: replaced/removed jars are renamed to {@code *.beacon-old} and swept at next start.
 * Blocking; call from the worker thread.
 */
final class HotPull {
    private HotPull() {}

    private static final Logger LOG = LoggerFactory.getLogger("BeaconSync");
    static final String RETIRED = ".beacon-old";

    record Result(int written, int removed, int modsChanged, boolean resourcesChanged, List<String> failures) {}

    static Result run(Repo repo, Hub hub, String target) throws IOException {
        Model.Tree next = tree(repo, hub, target);
        // Without the old tree every file gets a hash check and nothing can be known to be deleted.
        Map<String, Model.Entry> old = localTree(repo).map(HotPull::clientFiles).orElse(Map.of());
        Map<String, Model.Entry> wanted = clientFiles(next);

        // Files already on disk by hash, so renames and .disabled toggles copy instead of downloading.
        Map<String, Path> onDisk = new HashMap<>();
        old.forEach((path, entry) -> onDisk.putIfAbsent(entry.hash(), repo.gamePath(path)));

        int written = 0, removed = 0, mods = 0;
        boolean resources = false;
        List<String> failures = new ArrayList<>();

        for (var e : wanted.entrySet()) {
            String path = e.getKey();
            Model.Entry entry = e.getValue();
            try {
                Path file = repo.gamePath(path);
                if (upToDate(file, entry, old.get(path))) continue;
                fetch(hub, path, entry, file, onDisk.get(entry.hash()));
                written++;
                if (isMod(path)) mods++;
                if (isResource(path)) resources = true;
            } catch (IOException | RuntimeException ex) {
                failures.add(path + ": " + ex.getMessage());
            }
        }

        for (String path : old.keySet()) {
            if (wanted.containsKey(path)) continue;
            try {
                Path file = repo.gamePath(path);
                if (!Files.exists(file)) continue;
                if (isLoadedJar(path)) retire(file);
                else Files.delete(file);
                removed++;
                if (isMod(path)) mods++;
                if (isResource(path)) resources = true;
            } catch (IOException | RuntimeException ex) {
                failures.add(path + ": " + ex.getMessage());
            }
        }

        // Only move HEAD when the game dir really matches the target, so a retry re-diffs.
        if (failures.isEmpty()) {
            repo.setHead(target);
            repo.dropIndex();
        }
        return new Result(written, removed, mods, resources, failures);
    }

    /** Deletes jars retired by a previous hot pull. Call at startup, before they could matter. */
    static int sweepRetired(Path modsDir) throws IOException {
        if (!Files.isDirectory(modsDir)) return 0;
        int count = 0;
        try (var files = Files.list(modsDir)) {
            for (Path file : (Iterable<Path>) files::iterator) {
                if (file.getFileName().toString().endsWith(RETIRED) && Files.deleteIfExists(file)) count++;
            }
        }
        return count;
    }

    /** Tree of {@code commit}, from the local store or the hub (verified and cached locally). */
    private static Model.Tree tree(Repo repo, Hub hub, String commit) throws IOException {
        Model.Commit c = Repo.parse(object(repo, hub, commit), Model.Commit.class);
        if (c.tree() == null) throw new IOException("commit " + Hub.abbrev(commit) + " has no tree");
        Model.Tree tree = Repo.parse(object(repo, hub, c.tree()), Model.Tree.class);
        if (tree.files() == null) throw new IOException("tree " + Hub.abbrev(c.tree()) + " has no files");
        return tree;
    }

    private static String object(Repo repo, Hub hub, String hash) throws IOException {
        Optional<byte[]> local = repo.readObject(hash);
        byte[] bytes = local.isPresent() ? local.get() : hub.object(hash);
        if (local.isEmpty()) repo.writeObject(hash, bytes);
        return new String(bytes, StandardCharsets.UTF_8);
    }

    /** HEAD's tree from the local store only; empty when HEAD or its objects are missing. */
    private static Optional<Model.Tree> localTree(Repo repo) {
        try {
            Optional<String> head = repo.headCommit();
            if (head.isEmpty()) return Optional.empty();
            Optional<byte[]> commit = repo.readObject(head.get());
            if (commit.isEmpty()) return Optional.empty();
            String treeHash = Repo.parse(new String(commit.get(), StandardCharsets.UTF_8), Model.Commit.class).tree();
            if (treeHash == null) return Optional.empty();
            Optional<byte[]> tree = repo.readObject(treeHash);
            if (tree.isEmpty()) return Optional.empty();
            return Optional.of(Repo.parse(new String(tree.get(), StandardCharsets.UTF_8), Model.Tree.class))
                    .filter(t -> t.files() != null);
        } catch (IOException | RuntimeException e) {
            LOG.warn("Local tree unreadable, hashing files on disk instead", e);
            return Optional.empty();
        }
    }

    private static Map<String, Model.Entry> clientFiles(Model.Tree tree) {
        return tree.files().entrySet().stream()
                .filter(e -> e.getValue() != null && e.getValue().hash() != null && e.getValue().onClient())
                .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue));
    }

    /** Trusts the old tree when the hash is unchanged, otherwise hashes the file on disk. */
    private static boolean upToDate(Path file, Model.Entry entry, Model.Entry before) throws IOException {
        if (!Files.isRegularFile(file) || Files.size(file) != entry.size()) return false;
        if (before != null && entry.hash().equals(before.hash())) return true;
        return Repo.sha512(file).equals(entry.hash());
    }

    /** Writes {@code entry}'s bytes to {@code file} via a verified temp file. */
    private static void fetch(Hub hub, String path, Model.Entry entry, Path file, Path localCopy) throws IOException {
        Path tmp = Repo.tempFor(file);
        try {
            String hash = null;
            if (localCopy != null && Files.isRegularFile(localCopy) && !localCopy.equals(file)) {
                try (InputStream in = Files.newInputStream(localCopy); OutputStream out = Files.newOutputStream(tmp)) {
                    hash = Repo.sha512(in, out);
                }
            }
            if (!entry.hash().equals(hash)) {
                String url = entry.source() != null && entry.source().url() != null ? entry.source().url() : hub.objectUrl(entry.hash());
                hash = hub.download(url, tmp);
            }
            if (!entry.hash().equals(hash)) throw new IOException("SHA-512 mismatch");
            // A running game keeps its jars open; move the old one aside instead of overwriting it.
            if (Files.exists(file) && isLoadedJar(path)) retire(file);
            Repo.moveInto(tmp, file);
        } finally {
            Files.deleteIfExists(tmp);
        }
    }

    private static void retire(Path jar) throws IOException {
        Files.move(jar, jar.resolveSibling(jar.getFileName() + RETIRED), StandardCopyOption.REPLACE_EXISTING);
    }

    private static boolean isLoadedJar(String path) {
        return path.startsWith("mods/") && path.endsWith(".jar");
    }

    /** Anything that changes which mods load needs a restart, including .disabled toggles. */
    private static boolean isMod(String path) {
        return path.startsWith("mods/") && (path.endsWith(".jar") || path.endsWith(".jar.disabled"));
    }

    private static boolean isResource(String path) {
        return path.startsWith("resourcepacks/") || path.startsWith("assets/") || path.contains("/assets/");
    }
}
