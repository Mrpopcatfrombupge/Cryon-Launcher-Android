package dev.cryon.client;

import java.io.File;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import org.json.JSONObject;

/**
 * The add-ons of one instance, in the folders and files the game itself uses.
 *
 * Three kinds, because the game treats them differently and so must the launcher:
 * content mods in `cryon-mods`, which the game loads only when `cryon-mods.txt`
 * says `mod.<id>:true`; resource packs in `resourcepacks`, which the game picks
 * inside its own Options screen; shader packs in `shaderpacks`, likewise.
 *
 * The id and the description are read out of the archive itself — `cryon-mod.toml`
 * for a mod, `pack.mcmeta` for a pack — because a file called
 * `some-mod-1.2.zip` tells a player nothing, and the switch that turns it on has to
 * be written under the name the game will look for, not the name of the download.
 */
public final class Mods {
    public static final int CONTENT = 0;
    public static final int RESOURCE = 1;
    public static final int SHADER = 2;

    private static final String[] DIRECTORIES = {
        "cryon-mods", "resourcepacks", "shaderpacks",
    };
    private static final String[] TITLES = {"Mods", "Resource packs", "Shader packs"};

    public static final class Item {
        public File file;
        /** The game's name for it; the file's own name until a manifest says better. */
        public String id;
        public String title;
        public String description = "";
        public String version = "";
        public boolean enabled;
        /** Only a content mod has a switch the launcher can honestly reach. */
        public boolean toggleable;
        /** A mod built into the game: on unless the list says `false`. */
        public boolean nativeMod;

        public boolean isFolder() {
            return file.isDirectory();
        }
    }

    public static String title(int kind) {
        return TITLES[kind];
    }

    public static List<Item> list(File gameDir, int kind) {
        List<Item> found = new ArrayList<>();
        File[] files = new File(gameDir, DIRECTORIES[kind]).listFiles();
        if (files == null) {
            return found;
        }
        Map<String, String> switches = kind == CONTENT
                ? Cryon.readLines(new File(gameDir, "cryon-mods.txt"), ':')
                : new LinkedHashMap<String, String>();
        for (File file : files) {
            boolean archive = file.isFile()
                    && (file.getName().endsWith(".hmod") || file.getName().endsWith(".hnmod")
                            || file.getName().endsWith(".zip"));
            if (!file.isDirectory() && !archive) {
                continue;
            }
            Item item = new Item();
            item.file = file;
            item.id = stem(file.getName());
            item.title = item.id;
            item.toggleable = false;
            item.nativeMod = file.getName().endsWith(".hnmod");
            read(file, kind, item);
            if (kind == CONTENT) {
                try {
                    String manifest = text(file, "cryon-mod.toml");
                    boolean valid = manifest != null && item.id.matches("[a-z0-9_]{1,64}")
                            && !java.util.Arrays.asList("minecraft", "cryon", "c").contains(item.id);
                    item.toggleable = valid && !item.nativeMod && text(file, "mod.wasm") == null;
                    if (!item.toggleable) item.description = item.nativeMod
                            ? "Native source bundle: requires an Android-targeted engine rebuild."
                            : "Unsupported or invalid content package; it cannot be enabled here.";
                } catch (Exception invalid) { item.description = "Unreadable package: " + invalid.getMessage(); }
            }
            String state = switches.get("mod." + item.id);
            item.enabled = item.toggleable && "true".equals(state);
            found.add(item);
        }
        found.sort((a, b) -> a.title.compareToIgnoreCase(b.title));
        return found;
    }

    /** The name of a thing without its extension, which is what a mod's id usually is. */
    private static String stem(String name) {
        int dot = name.lastIndexOf('.');
        return dot > 0 ? name.substring(0, dot) : name;
    }

    /**
     * Pull the two lines a list wants out of the archive.
     *
     * A mod that cannot be read is still listed under its file name: the player put
     * it there, and a launcher that hides it would look like it had been deleted.
     */
    private static void read(File file, int kind, Item item) {
        try {
            if (kind == CONTENT) {
                toml(text(file, "cryon-mod.toml"), item);
            } else {
                json(text(file, "pack.mcmeta"), item);
            }
        } catch (Exception unreadable) {
            // The file name is the answer, and it is already in item.
        }
    }

    /** One file out of a folder or a zip, or null when the archive has none. */
    private static String text(File file, String entryName) throws Exception {
        if (file.isDirectory()) {
            File entry = new File(file, entryName);
            if (!entry.isFile()) {
                return null;
            }
            if (entry.length() > (1 << 20)) throw new java.io.IOException("Metadata exceeds 1 MiB");
            return new String(java.nio.file.Files.readAllBytes(entry.toPath()),
                    java.nio.charset.StandardCharsets.UTF_8);
        }
        try (ZipFile zip = new ZipFile(file)) {
            ZipEntry entry = zip.getEntry(entryName);
            if (entry == null) {
                return null;
            }
            if (entry.getSize() > (1 << 20)) throw new java.io.IOException("Metadata exceeds 1 MiB");
            try (InputStream in = zip.getInputStream(entry)) {
                java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
                byte[] buffer = new byte[4096];
                for (int read = in.read(buffer); read > 0; read = in.read(buffer)) {
                    if (bytes.size() + read > (1 << 20)) throw new java.io.IOException("Metadata exceeds 1 MiB");
                    bytes.write(buffer, 0, read);
                }
                return bytes.toString("UTF-8");
            }
        }
    }

    /** `id = "cryon-example"` and `description = "..."`, which is all of TOML this needs. */
    private static void toml(String text, Item item) {
        if (text == null) {
            return;
        }
        for (String line : text.split("\n")) {
            String[] pair = assignment(line);
            if (pair == null) {
                continue;
            }
            if (pair[0].equals("id")) {
                item.id = pair[1];
                item.title = pair[1];
            } else if (pair[0].equals("name") && !pair[1].isEmpty()) {
                item.title = pair[1];
            } else if (pair[0].equals("version")) {
                item.version = pair[1];
            } else if (pair[0].equals("description")) {
                item.description = pair[1];
            }
        }
    }

    private static String[] assignment(String line) {
        int equals = line.indexOf('=');
        if (equals <= 0) {
            return null;
        }
        String key = line.substring(0, equals).trim();
        String value = line.substring(equals + 1).trim();
        if (value.startsWith("\"") && value.length() > 1) {
            int end = value.lastIndexOf('"');
            value = end > 0 ? value.substring(1, end) : value.substring(1);
        }
        return new String[] {key, value};
    }

    /** Vanilla's `pack.mcmeta`: `{"pack": {"description": ..., "supported_formats": ...}}`. */
    private static void json(String text, Item item) {
        if (text == null) {
            return;
        }
        try {
            JSONObject pack = new JSONObject(text).optJSONObject("pack");
            if (pack == null) {
                return;
            }
            Object description = pack.opt("description");
            item.description = description instanceof String
                    ? (String) description : description == null ? "" : description.toString();
        } catch (Exception notJson) {
            // A pack whose metadata does not parse is still a pack on disk.
        }
    }

    /**
     * Turn a content mod on or off, in the game's own list.
     *
     * Written with the engine's separator and merged into what is already there, so
     * a player's key choices for other mods survive the change.
     */
    public static void setEnabled(File gameDir, Item item, boolean on) throws Exception {
        File list = new File(gameDir, "cryon-mods.txt");
        if (!item.toggleable || !item.id.matches("[a-z0-9_]{1,64}")) throw new java.io.IOException("Unsupported mod");
        Map<String, String> lines = Cryon.readLinesChecked(list, ':');
        lines.put("mod." + item.id, String.valueOf(on));
        Cryon.writeLines(list, ':', lines);
    }

    /** How much of the instance these add-ons take, which is the reason to delete one. */
    public static String size(Item item) {
        return Worlds.size(item.file);
    }

    /** One `Label:  value` line of a details dialog, skipped when there is nothing. */
    public static void line(StringBuilder into, String label, String value) {
        if (value != null && !value.isEmpty()) {
            into.append(label).append(":  ").append(value).append('\n');
        }
    }

    private Mods() {
    }
}
