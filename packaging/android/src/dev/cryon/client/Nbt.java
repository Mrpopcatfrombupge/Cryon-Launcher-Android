package dev.cryon.client;

import java.io.DataInputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.zip.GZIPInputStream;

/**
 * The little bit of NBT this launcher needs.
 *
 * The game keeps its descriptions of saved things — a world's name, the server
 * list, a resource pack's `pack.mcmeta` aside — in NBT: a tree of named tags,
 * gzipped. Reading the whole format would be a dependency and a data model, and
 * nothing here needs either: a launcher shows a dozen fields and no more.
 *
 * So this parses it straight into Maps, Lists and boxed numbers, and stops. No
 * writing: a launcher that edits a save file can damage a save, and every field it
 * currently wants to change is one the game writes better itself.
 */
public final class Nbt {
    /** Tag type ids, as the format defines them. */
    private static final int END = 0, BYTE = 1, SHORT = 2, INT = 3, LONG = 4,
            FLOAT = 5, DOUBLE = 6, BYTES = 7, STRING = 8, LIST = 9, COMPOUND = 10,
            INTS = 11, LONGS = 12;

    /**
     * The root compound of a save file, gzipped or not.
     *
     * Uncompressed `level.dat` exists in the wild — an older save, or a folder
     * copied in from a tool that did not know better — and a launcher that refuses
     * to look at one shows an empty worlds list over files that are plainly there.
     */
    public static Map<String, Object> read(File file) throws IOException {
        InputStream opened = new FileInputStream(file);
        DataInputStream in;
        try {
            in = new DataInputStream(new GZIPInputStream(opened));
        } catch (IOException notGzipped) {
            opened.close();
            in = new DataInputStream(new FileInputStream(file));
        }
        try {
            int type = in.readUnsignedByte();
            if (type != COMPOUND) {
                throw new IOException(file.getName() + " is not a compound");
            }
            name(in);
            Object root = value(in, COMPOUND);
            return asCompound(root);
        } finally {
            in.close();
        }
    }

    private static String name(DataInputStream in) throws IOException {
        byte[] text = new byte[in.readUnsignedShort()];
        in.readFully(text);
        return new String(text, StandardCharsets.UTF_8);
    }

    private static Object value(DataInputStream in, int type) throws IOException {
        switch (type) {
            case BYTE:
                return in.readByte();
            case SHORT:
                return in.readShort();
            case INT:
                return in.readInt();
            case LONG:
                return in.readLong();
            case FLOAT:
                return in.readFloat();
            case DOUBLE:
                return in.readDouble();
            case BYTES: {
                byte[] bytes = new byte[in.readInt()];
                in.readFully(bytes);
                return bytes;
            }
            case STRING:
                return name(in);
            case LIST: {
                int element = in.readUnsignedByte();
                int count = in.readInt();
                List<Object> list = new ArrayList<>(Math.min(count, 4096));
                for (int i = 0; i < count; i++) {
                    list.add(value(in, element));
                }
                return list;
            }
            case COMPOUND: {
                Map<String, Object> compound = new LinkedHashMap<>();
                while (true) {
                    int childType = in.readUnsignedByte();
                    if (childType == END) {
                        return compound;
                    }
                    String childName = name(in);
                    compound.put(childName, value(in, childType));
                }
            }
            case INTS: {
                int[] values = new int[in.readInt()];
                for (int i = 0; i < values.length; i++) {
                    values[i] = in.readInt();
                }
                return values;
            }
            case LONGS: {
                long[] values = new long[in.readInt()];
                for (int i = 0; i < values.length; i++) {
                    values[i] = in.readLong();
                }
                return values;
            }
            default:
                throw new IOException("unknown NBT tag " + type);
        }
    }

    /** An empty map when the tag is missing or is not a compound. */
    @SuppressWarnings("unchecked")
    public static Map<String, Object> asCompound(Object value) {
        return value instanceof Map ? (Map<String, Object>) value
                : new LinkedHashMap<String, Object>();
    }

    /** The named compound inside a compound. */
    public static Map<String, Object> child(Map<String, Object> compound, String key) {
        return asCompound(compound.get(key));
    }

    /** An empty list when the tag is missing or is not a list. */
    @SuppressWarnings("unchecked")
    public static List<Object> asList(Object value) {
        return value instanceof List ? (List<Object>) value : new ArrayList<Object>();
    }

    /** Text, or the fallback when the tag is absent — an unnamed world is still a world. */
    public static String text(Object value, String fallback) {
        return value instanceof String && !((String) value).isEmpty()
                ? (String) value : fallback;
    }

    /** A number of any tag width, or the fallback. Vanilla widens these over the years. */
    public static long number(Object value, long fallback) {
        return value instanceof Number ? ((Number) value).longValue() : fallback;
    }

    private Nbt() {
    }
}
