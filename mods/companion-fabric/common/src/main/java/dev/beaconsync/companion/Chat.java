package dev.beaconsync.companion;

import java.util.ArrayList;
import java.util.List;

/**
 * A chat line without Minecraft types, so the logic that builds it is shared by every target.
 * Each {@link Platform} renders it: the first span is the root component and later spans are
 * appended to it, inheriting the root's style like vanilla components do.
 */
record Chat(List<Span> spans) {
    static final String PULL_COMMAND = "/beacon pull";

    enum Tone {
        /** Inherits the root style. */
        PLAIN,
        /** The aqua "[BeaconSync] " prefix. */
        PREFIX,
        OK,
        WARN,
        ERROR,
        LABEL,
        VALUE,
        BOLD,
        /** Green underlined link that runs {@link #PULL_COMMAND}. */
        PULL_LINK
    }

    record Span(String text, Tone tone) {}

    /** {@code [BeaconSync] } followed by {@code text} in {@code tone}. */
    static Chat prefixed(Tone tone, String text) {
        return new Chat(List.of(new Span("[BeaconSync] ", Tone.PREFIX))).and(tone, text);
    }

    static Chat error(String text) {
        return prefixed(Tone.ERROR, text);
    }

    /** {@code Label: value} row used by /beacon status. */
    static Chat row(String label, String value) {
        return new Chat(List.of(new Span(label + ": ", Tone.LABEL), new Span(value, Tone.VALUE)));
    }

    Chat and(Tone tone, String text) {
        List<Span> next = new ArrayList<>(spans);
        next.add(new Span(text, tone));
        return new Chat(List.copyOf(next));
    }

    Chat pullLink() {
        return and(Tone.PULL_LINK, "[Update]");
    }
}
