package dev.cryon.client;

import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * The worlds in an instance, and what the game saved for each one.
 *
 * A world is a folder under the instance's `saves`, and the interesting things about
 * it are in `level.dat` — the name the player gave it, how long they have played
 * there, when they last were, and what kind of save it is. `icon.png` beside it is
 * the picture the game took of the world when it saved, which is why a list of
 * worlds should show it: a name says which folder, a picture says which world.
 */
public final class Worlds {
    public static final class World {
        public File dir;
        public String name;
        /** Millis since the epoch; the folder's own time when the save says nothing. */
        public long lastPlayed;
        public long playTimeTicks;
        /** Vanilla's `GameType`: 0 survival, 1 creative, 2 adventure, 3 spectator, -1 unknown. */
        public int gameType = -1;
        public boolean hardcore;
        /** The seed, for the worlds that stamp one; null when the save says none. */
        public Long seed;
        /** The version the save was last opened by, e.g. `26.3`. */
        public String version = "";
        public File iconFile;
        /** The folder is a world but `level.dat` did not read. */
        public boolean broken;

        public boolean hasIcon() {
            return iconFile != null && iconFile.isFile();
        }

        public String mode() {
            if (hardcore) {
                return "Hardcore";
            }
            switch (gameType) {
                case 0:
                    return "Survival";
                case 1:
                    return "Creative";
                case 2:
                    return "Adventure";
                case 3:
                    return "Spectator";
                default:
                    return "";
            }
        }
    }

    /** Every world in the instance's saves folder, the ones played last first. */
    public static List<World> list(File gameDir) {
        List<World> found = new ArrayList<>();
        File[] folders = new File(gameDir, "saves").listFiles(File::isDirectory);
        if (folders == null) {
            return found;
        }
        for (File folder : folders) {
            File data = new File(folder, "level.dat");
            if (!data.isFile()) {
                // Not a save: a temp folder the game left behind while copying one in.
                continue;
            }
            World world = new World();
            world.dir = folder;
            world.name = folder.getName();
            world.iconFile = new File(folder, "icon.png");
            try {
                fill(data, world);
            } catch (Exception unreadable) {
                world.broken = true;
            }
            if (world.lastPlayed <= 0) {
                world.lastPlayed = folder.lastModified();
            }
            found.add(world);
        }
        found.sort((a, b) -> Long.compare(b.lastPlayed, a.lastPlayed));
        return found;
    }

    private static void fill(File data, World world) throws Exception {
        Map<String, Object> root = Nbt.read(data);
        Map<String, Object> level = Nbt.child(root, "Data");
        world.name = Nbt.text(level.get("LevelName"), world.name);
        world.lastPlayed = Nbt.number(level.get("LastPlayed"), 0);
        world.playTimeTicks = Nbt.number(level.get("Time"), 0);
        world.gameType = (int) Nbt.number(level.get("GameType"), -1);
        world.hardcore = Nbt.number(level.get("hardcore"), 0) != 0;
        world.version = Nbt.text(Nbt.child(level, "Version").get("Name"), "");
        Map<String, Object> generation = Nbt.child(level, "WorldGenSettings");
        if (generation.get("seed") instanceof Number) {
            world.seed = ((Number) generation.get("seed")).longValue();
        }
    }

    /** How long the player has spent there, in the words a list can show. */
    public static String playTime(long ticks) {
        long seconds = ticks / 20;
        long hours = seconds / 3600;
        long minutes = (seconds % 3600) / 60;
        return hours > 0 ? hours + "h " + minutes + "m" : minutes + "m";
    }

    /** `LastPlayed` as the date a player recognises, or "Never" if there is none. */
    public static String playedOn(long millis) {
        long then = System.currentTimeMillis() - millis;
        if (millis <= 0) {
            return "Never";
        }
        if (then < 60_000) {
            return "Just now";
        }
        if (then < 3_600_000) {
            return (then / 60_000) + "m ago";
        }
        if (then < 86_400_000) {
            return (then / 3_600_000) + "h ago";
        }
        if (then < 7 * 86_400_000L) {
            return (then / 86_400_000L) + "d ago";
        }
        return java.text.DateFormat.getDateInstance(java.text.DateFormat.MEDIUM)
                .format(new java.util.Date(millis));
    }

    /** The whole world folder, the way a player thinks of a world's size. */
    public static String size(File folder) {
        return humanSize(bytesOf(folder));
    }

    /** A byte count in the units a phone shows. */
    public static String humanSize(long bytes) {
        if (bytes < 1024) {
            return bytes + " B";
        }
        if (bytes < 1024 * 1024) {
            return bytes / 1024 + " KiB";
        }
        if (bytes < 1024L * 1024 * 1024) {
            return bytes / (1024 * 1024) + " MiB";
        }
        return String.format(java.util.Locale.US, "%.1f GiB",
                bytes / (1024.0 * 1024 * 1024));
    }

    private static long bytesOf(File file) {
        if (file.isFile()) {
            return file.length();
        }
        File[] kids = file.listFiles();
        if (kids == null) {
            return 0;
        }
        long total = 0;
        for (File kid : kids) {
            total += bytesOf(kid);
        }
        return total;
    }

    private Worlds() {
    }
}
