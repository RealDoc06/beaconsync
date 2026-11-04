package dev.beaconsync.companion;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;

/**
 * Minimal server list ping. A Minecraft server only answers status requests once its world is
 * loaded, so a reply means it is ready for players (its port opens earlier than that).
 */
final class Ping {
    private Ping() {}

    static boolean ready(String host, int port) {
        try (Socket socket = new Socket()) {
            socket.connect(new InetSocketAddress(host, port), 3000);
            socket.setSoTimeout(3000);
            DataOutputStream out = new DataOutputStream(socket.getOutputStream());
            ByteArrayOutputStream handshake = new ByteArrayOutputStream();
            DataOutputStream h = new DataOutputStream(handshake);
            writeVarInt(h, 0x00); // handshake packet
            writeVarInt(h, -1); // any protocol version may ask for status
            byte[] hostBytes = host.getBytes(StandardCharsets.UTF_8);
            writeVarInt(h, hostBytes.length);
            h.write(hostBytes);
            h.writeShort(port);
            writeVarInt(h, 1); // next state: status
            writeVarInt(out, handshake.size());
            out.write(handshake.toByteArray());
            out.write(new byte[] {1, 0x00}); // status request
            out.flush();
            DataInputStream in = new DataInputStream(socket.getInputStream());
            int length = readVarInt(in);
            return length > 1 && readVarInt(in) == 0x00;
        } catch (IOException e) {
            return false;
        }
    }

    private static void writeVarInt(DataOutputStream out, int value) throws IOException {
        while ((value & ~0x7F) != 0) {
            out.writeByte((value & 0x7F) | 0x80);
            value >>>= 7;
        }
        out.writeByte(value);
    }

    private static int readVarInt(InputStream in) throws IOException {
        int value = 0;
        for (int shift = 0; shift < 35; shift += 7) {
            int b = in.read();
            if (b < 0) throw new IOException("closed");
            value |= (b & 0x7F) << shift;
            if ((b & 0x80) == 0) return value;
        }
        throw new IOException("varint too long");
    }
}
