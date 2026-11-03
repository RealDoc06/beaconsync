package dev.beaconsync.companion;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.URI;
import java.net.URLEncoder;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;

/** Minimal client for the hub's public API. All calls block; run them off the render thread. */
final class Hub {
    private static final HttpClient HTTP = HttpClient.newBuilder()
            .connectTimeout(Duration.ofSeconds(10))
            .followRedirects(HttpClient.Redirect.NORMAL)
            .build();

    private final String base;
    private final String userAgent;

    Hub(String hubUrl, String userAgent) {
        this.base = hubUrl.replaceAll("/+$", "");
        this.userAgent = userAgent;
    }

    Model.Pack pack(String code) throws IOException {
        byte[] body = get(base + "/api/public/packs/" + URLEncoder.encode(code, StandardCharsets.UTF_8), Duration.ofSeconds(20));
        return Repo.parse(new String(body, StandardCharsets.UTF_8), Model.Pack.class);
    }

    /** Fetches an object from the hub store and checks it hashes to {@code hash}. */
    byte[] object(String hash) throws IOException {
        byte[] bytes = get(objectUrl(hash), Duration.ofSeconds(60));
        String actual = Repo.sha512(bytes);
        if (!actual.equals(hash)) throw new IOException("object " + abbrev(hash) + " failed SHA-512 check");
        return bytes;
    }

    String objectUrl(String hash) {
        return base + "/api/objects/" + hash;
    }

    /** Streams {@code url} into {@code file} and returns the SHA-512 of what was written. */
    String download(String url, Path file) throws IOException {
        HttpResponse<InputStream> res = send(url, Duration.ofMinutes(5), HttpResponse.BodyHandlers.ofInputStream());
        try (InputStream in = res.body(); OutputStream out = Files.newOutputStream(file)) {
            return Repo.sha512(in, out);
        }
    }

    private byte[] get(String url, Duration timeout) throws IOException {
        return send(url, timeout, HttpResponse.BodyHandlers.ofByteArray()).body();
    }

    private <T> HttpResponse<T> send(String url, Duration timeout, HttpResponse.BodyHandler<T> handler) throws IOException {
        HttpRequest req = HttpRequest.newBuilder(URI.create(url))
                .timeout(timeout)
                .header("User-Agent", userAgent)
                .GET()
                .build();
        HttpResponse<T> res;
        try {
            res = HTTP.send(req, handler);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new IOException("interrupted", e);
        }
        if (res.statusCode() / 100 != 2) {
            if (res.body() instanceof InputStream in) in.close();
            throw new IOException("HTTP " + res.statusCode() + " from " + url);
        }
        return res;
    }

    static String abbrev(String hash) {
        return hash == null ? "none" : hash.substring(0, Math.min(8, hash.length()));
    }
}
