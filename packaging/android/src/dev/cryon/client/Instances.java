package dev.cryon.client;

import java.io.File;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * The instances on the device.
 *
 * An instance is a folder the game writes into: its own `options.txt`, its own
 * control layout, its own worlds, mods and resource packs, and its own engine if
 * the versions differ. That is the whole feature, and the engine already supports
 * it — `CRYON_GAME_DIR` says which folder is the game's home, so switching instance
 * is one line of `env.txt` rather than a second copy of anything.
 *
 * The instance that existed before this screen, `files/game`, stays the default and
 * is never deleted, because it holds the settings and worlds the player already made.
 */
public final class Instances {
    /** The folder that has always been the game's home. */
    public static final String DEFAULT = "game";

    private Instances() {
    }

    /** Where the instances other than the default live. */
    private static File root(File files) {
        return new File(files, "instances");
    }

    /** The instance's folder: the default at `files/game`, the rest under `instances`. */
    public static File dir(File files, String name) {
        return name.equals(DEFAULT)
                ? new File(files, DEFAULT)
                : new File(root(files), name);
    }

    /** Every instance, the default first, then the rest by name. */
    public static List<String> list(File files) {
        List<String> names = new ArrayList<>();
        names.add(DEFAULT);
        File[] folders = root(files).listFiles(File::isDirectory);
        if (folders != null) {
            for (File folder : folders) {
                if (!folder.getName().equals(DEFAULT)) {
                    names.add(folder.getName());
                }
            }
        }
        Collections.sort(names, (a, b) -> a.equals(DEFAULT) ? -1
                : b.equals(DEFAULT) ? 1 : a.compareToIgnoreCase(b));
        return names;
    }

    /** The instance PLAY will use, remembered between runs. */
    public static String current(File files) {
        File marker = new File(files, "instance.txt");
        if (!marker.isFile()) {
            return DEFAULT;
        }
        try {
            String name = new String(java.nio.file.Files.readAllBytes(marker.toPath()),
                    java.nio.charset.StandardCharsets.UTF_8).trim();
            if (name.isEmpty()) {
                return DEFAULT;
            }
            return dir(files, name).isDirectory() ? name : DEFAULT;
        } catch (Exception unreadable) {
            return DEFAULT;
        }
    }

    public static void setCurrent(File files, String name) throws java.io.IOException {
        Cryon.write(new File(files, "instance.txt"), name + "\n");
    }

    /**
     * Make a new instance. It starts empty on purpose: a copy of the default's
     * settings would carry over a world the player did not ask for, and an empty
     * folder is what "a separate game" means. The game writes its own defaults into
     * it the first time it runs there.
     */
    public static boolean create(File files, String name) {
        String clean = sane(name);
        if (clean == null) {
            return false;
        }
        File target = dir(files, clean);
        if (target.exists()) {
            return false;
        }
        return (clean.equals(DEFAULT) || target.mkdirs()) && target.isDirectory();
    }

    /**
     * Rename one. The folder is the instance, so the name is the folder, and the
     * current-instance marker is rewritten if it pointed at the old name.
     */
    public static boolean rename(File files, String from, String to) {
        String clean = sane(to);
        if (clean == null || from.equals(clean)) {
            return false;
        }
        File source = dir(files, from);
        File target = dir(files, clean);
        if (!source.isDirectory() || target.exists()) {
            return false;
        }
        // current() falls back to the default once the old folder is gone.
        boolean wasCurrent = current(files).equals(from);
        if (!source.renameTo(target)) {
            return false;
        }
        try {
            if (wasCurrent) {
                setCurrent(files, clean);
            }
        } catch (java.io.IOException ignored) {
            // The rename worked, which is the thing the player asked for.
        }
        return true;
    }

    /**
     * Delete one, with its settings, worlds and mods.
     *
     * The default instance cannot be deleted: it is where the player's first worlds
     * are, and an app that offers to erase the only copy of them with one tap has
     * gone too far.
     */
    public static boolean delete(File files, String name) {
        if (name.equals(DEFAULT)) {
            return false;
        }
        File target = dir(files, name);
        if (!target.isDirectory()) {
            return false;
        }
        Cryon.deleteAll(target);
        boolean gone = !target.exists();
        if (gone) {
            try {
                if (current(files).equals(name)) {
                    setCurrent(files, DEFAULT);
                }
            } catch (java.io.IOException ignored) {
                // The instance is gone; the marker will fall back to the default.
            }
        }
        return gone;
    }

    /** A name a folder can actually hold: no path, no reserved characters. */
    private static String sane(String name) {
        if (name == null) {
            return null;
        }
        String clean = name.trim().replaceAll("[/\\\\:*?\"<>|]", "-");
        return clean.isEmpty() || clean.equals(".") || clean.equals("..") ? null : clean;
    }

    private static void deleteAll(File file) {
        File[] kids = file.listFiles();
        if (kids != null) {
            for (File kid : kids) {
                deleteAll(kid);
            }
        }
        file.delete();
    }
}
