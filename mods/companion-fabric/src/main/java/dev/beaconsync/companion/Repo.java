package dev.beaconsync.companion;

import com.google.gson.Gson;
import com.google.gson.JsonParseException;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.DigestInputStream;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.Optional;
import java.util.regex.Pattern;

/**
 * Read/write access to an instance's {@code .beacon/} repo and its {@code minecraft/} game dir.
 * Construct with {@link #find(Path)} from the game dir; every method does blocking IO.
 */
final class Repo {
    static final Gson GSON = new Gson();
    private static final Pattern HASH = Pattern.compile("[0-9a-f]{128}");
    private static final String REF_PREFIX = "ref: ";

    final Path gameDir;
    final Path beaconDir;

    private Repo(Path gameDir) {
        this.gameDir = gameDir.toAbsolutePath().normalize();
        this.beaconDir = this.gameDir.getParent().resolve(".beacon");
    }

    /** The repo for this game dir, or empty when the instance is not a BeaconSync pack. */
    static Optional<Repo> find(Path gameDir) {
        Repo repo = new Repo(gameDir);
        return Files.isRegularFile(repo.beaconDir.resolve("config.json")) ? Optional.of(repo) : Optional.empty();
    }

    Model.Config config() throws IOException {
        return parse(Files.readString(beaconDir.resolve("config.json")), Model.Config.class);
    }

    /** Commit HEAD points at, following one level of {@code ref:}. Empty for a fresh repo. */
    Optional<String> headCommit() throws IOException {
        Path head = headTarget();
        if (!Files.exists(head)) return Optional.empty();
        String value = Files.readString(head).trim();
        return value.isEmpty() ? Optional.empty() : Optional.of(value);
    }

    /** Points the current branch (or a detached HEAD) at {@code commit}. */
    void setHead(String commit) throws IOException {
        Path target = headTarget();
        Files.createDirectories(target.getParent());
        writeAtomic(target, (commit + "\n").getBytes(StandardCharsets.UTF_8));
    }

    /** File holding the commit hash: the branch ref file, or HEAD itself when detached. */
    private Path headTarget() throws IOException {
        Path head = beaconDir.resolve("HEAD");
        if (!Files.exists(head)) return head;
        String value = Files.readString(head).trim();
        if (!value.startsWith(REF_PREFIX)) return head;
        return inside(beaconDir, value.substring(REF_PREFIX.length()).trim());
    }

    Path objectPath(String hash) {
        if (!HASH.matcher(hash).matches()) throw new IllegalArgumentException("bad object id " + hash);
        return beaconDir.resolve("objects").resolve(hash.substring(0, 2)).resolve(hash.substring(2));
    }

    Optional<byte[]> readObject(String hash) throws IOException {
        Path path = objectPath(hash);
        return Files.isRegularFile(path) ? Optional.of(Files.readAllBytes(path)) : Optional.empty();
    }

    void writeObject(String hash, byte[] bytes) throws IOException {
        Path path = objectPath(hash);
        if (Files.exists(path)) return;
        Files.createDirectories(path.getParent());
        writeAtomic(path, bytes);
    }

    /** Resolves a tree path under the game dir, rejecting anything that escapes it. */
    Path gamePath(String treePath) {
        return inside(gameDir, treePath);
    }

    /** The stat cache is stale after we touch files; the desktop app rebuilds it when missing. */
    void dropIndex() throws IOException {
        Files.deleteIfExists(beaconDir.resolve("index.json"));
    }

    static <T> T parse(String json, Class<T> type) throws IOException {
        try {
            T value = GSON.fromJson(json, type);
            if (value == null) throw new IOException("empty " + type.getSimpleName());
            return value;
        } catch (JsonParseException e) {
            throw new IOException("invalid " + type.getSimpleName() + " JSON: " + e.getMessage(), e);
        }
    }

    static Path inside(Path root, String relative) {
        Path resolved = root.resolve(relative).normalize();
        if (!resolved.startsWith(root) || resolved.equals(root)) {
            throw new IllegalArgumentException("path escapes " + root + ": " + relative);
        }
        return resolved;
    }

    static String sha512(byte[] bytes) {
        return HexFormat.of().formatHex(digest().digest(bytes));
    }

    static String sha512(Path file) throws IOException {
        try (InputStream in = Files.newInputStream(file)) {
            return sha512(in, OutputStream.nullOutputStream());
        }
    }

    /** Copies {@code in} to {@code out} and returns the SHA-512 of the bytes copied. */
    static String sha512(InputStream in, OutputStream out) throws IOException {
        DigestInputStream digesting = new DigestInputStream(in, digest());
        digesting.transferTo(out);
        return HexFormat.of().formatHex(digesting.getMessageDigest().digest());
    }

    static void writeAtomic(Path target, byte[] bytes) throws IOException {
        Path tmp = tempFor(target);
        Files.write(tmp, bytes);
        moveInto(tmp, target);
    }

    /** Sibling temp file, so the final move stays on one filesystem. */
    static Path tempFor(Path target) throws IOException {
        Files.createDirectories(target.getParent());
        return Files.createTempFile(target.getParent(), "." + target.getFileName(), ".beacon-tmp");
    }

    static void moveInto(Path tmp, Path target) throws IOException {
        try {
            Files.move(tmp, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (AtomicMoveNotSupportedException e) {
            Files.move(tmp, target, StandardCopyOption.REPLACE_EXISTING);
        } finally {
            Files.deleteIfExists(tmp);
        }
    }

    private static MessageDigest digest() {
        try {
            return MessageDigest.getInstance("SHA-512");
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }
}
