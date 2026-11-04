package dev.cryon.client;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The files this app and the game share, and the only way the two talk.
 *
 * The game is a Rust library loaded by the platform's NativeActivity, and a
 * phone gives an app no command line, so the launcher's whole message to it is
 * one file: `files/env.txt`, one KEY=VALUE per line, which the game's own start
 * reads before it opens a window. That is the handoff — pick a version here,
 * write the two lines that say so, start the activity, and the game loads that
 * version without showing a chooser of its own.
 *
 * The directory is the app's own external files dir, which is what the game
 * already uses: it is readable and writable without any permission, and a phone
 * with a cable in it can copy game files in and out by hand.
 */
public final class Cryon {
    /** The activity that loads the engine, as the manifest names it. */
    public static final String GAME_ACTIVITY = "android.app.NativeActivity";

    private final File files;
    private final File versions;

    private Cryon(File files) {
        this.files = files;
        this.versions = new File(files, "versions");
    }

    /** The app's own directory, as the game sees it too. */
    public static Cryon open(Context context) {
        File dir = context.getExternalFilesDir(null);
        if (dir == null) {
            // No external storage — a private dir still runs the game, only
            // without the convenience of a file manager.
            dir = new File(context.getFilesDir(), "files");
        }
        if (!dir.isDirectory() && !dir.mkdirs()) {
            throw new IllegalStateException("cannot create " + dir.getAbsolutePath());
        }
        return new Cryon(dir);
    }

    public File files() {
        return files;
    }

    /**
     * The folder the game reads and writes: the chosen instance's.
     *
     * Asked for each time rather than remembered at startup, because the instance
     * changes while the app is open — on the Instances screen, or by tapping a world
     * that lives in another instance — and a launcher that kept launching the folder
     * it first saw would open the wrong game.
     */
    public File gameDir() {
        return Instances.dir(files, Instances.current(files));
    }

    /** The switch file, which the game reads before it opens a window. */
    public File envFile() {
        return new File(files, "env.txt");
    }

    /**
     * The launcher's own preferences, which the game never reads.
     *
     * Kept apart from `env.txt` on purpose: that file is a message to the game, and
     * this one is a note to itself. Mixing them means the game is handed keys it does
     * not know and a player cannot tell which file to delete to get back to normal.
     */
    public File launcherFile() {
        return new File(files, "launcher.txt");
    }

    /** One of the launcher's own preferences, as the file says it. */
    public String preference(String key, String whenAbsent) {
        String value = readLines(launcherFile(), '=').get(key);
        return value == null || value.trim().isEmpty() ? whenAbsent : value.trim();
    }

    /**
     * Write one of the launcher's own preferences.
     *
     * Merged rather than rewritten: the file holds a player's other choices, and
     * saving one of them here must not be the reason the others are gone.
     */
    public void setPreference(String key, String value) throws IOException {
        Map<String, String> lines = readLinesChecked(launcherFile(), '=');
        lines.put(key, value);
        StringBuilder text = new StringBuilder();
        for (Map.Entry<String, String> line : lines.entrySet()) {
            text.append(line.getKey()).append('=').append(line.getValue()).append('\n');
        }
        write(launcherFile(), text.toString());
    }

    /**
     * The instance in use, by name. The default is called `game` because that is
     * the folder it has always been, and showing the folder's name is honest in a
     * way that inventing a friendlier label would not be.
     */
    public String instance() {
        return Instances.current(files);
    }

    /** The instance's folder, which is the game's home directory. */
    public File instanceDir() {
        return gameDir();
    }

    /** One `versions/*` folder, and whether the game could actually load it. */
    public static final class Version {
        public final String label;
        public final File dir;
        public final boolean playable;
        /** "" when the game can load this folder: otherwise the first part it lacks. */
        public final String missing;

        Version(String label, File dir, String missing) {
            this.label = label;
            this.dir = dir;
            this.missing = missing;
            this.playable = missing.isEmpty();
        }

        @Override
        public String toString() {
            return label;
        }
    }

    /**
     * The parts of a prepared version that the engine opens before it can draw a
     * frame. Copying a version onto a phone in pieces is the failure that actually
     * happens — a transfer stops halfway — and the game then says only that some
     * file was not there. Naming the part here means the row says what to copy.
     */
    private static final String[] NEEDED = {
        "cryon-resources.json", "registries.json", "facts.json", "model-states.json",
        "render-definitions.json", "assets/minecraft/blockstates", "data", "worldgen",
    };

    /** "" when every part is present, or the first one that is not. */
    public static String missing(File dir) {
        for (String needed : NEEDED) {
            if (!new File(dir, needed).exists()) {
                return needed;
            }
        }
        return "";
    }

    /**
     * What is installed, newest label first.
     *
     * A folder counts as playable only if the engine can load it (see NEEDED).
     * An incomplete folder is still listed — a person who copied a version in and
     * sees nothing will think the launcher is broken, whereas one dimmed row says
     * exactly what is missing.
     */
    public List<Version> versions() {
        List<Version> found = new ArrayList<>();
        File[] folders = versions.listFiles(File::isDirectory);
        if (folders != null) {
            for (File folder : folders) {
                found.add(new Version(folder.getName(), folder, missing(folder)));
            }
        }
        Collections.sort(found, (a, b) -> b.label.compareTo(a.label));
        return found;
    }

    /**
     * Writes the start the game will read, and returns what it pointed at.
     *
     * Three lines do the work: which folder of prepared files to load, and which
     * instance's directory to read and write the player's own files in. Anything
     * else the launcher has decided — the graphics settings, the sound and cutout
     * switches — is already in these files, so they are merged rather than
     * rewritten: PLAY must not undo a setting made on the settings screen.
     */
    public String writeStart(String resources, List<String> switches) throws IOException {
        File game = gameDir();
        Map<String, String> lines = readLinesChecked(envFile(), '=');
        lines.put("CRYON_RESOURCES", resources);
        lines.put("CRYON_GAME_DIR", game.getAbsolutePath());
        lines.put("CRYON_HOME", game.getAbsolutePath());
        // A world named by the Worlds screen is a one-shot instruction: PLAY means
        // the title screen unless a switch says otherwise, and a stale line here
        // would reopen last session's save without being asked.
        lines.remove("CRYON_WORLD");
        lines.remove("CRYON_SERVER");
        if (switches != null) {
            for (String line : switches) {
                int at = line.indexOf('=');
                if (at > 0) {
                    lines.put(line.substring(0, at).trim(), line.substring(at + 1).trim());
                }
            }
        }
        writeLines(envFile(), '=', lines);
        // The engine's own version choice reads this too, so a start that skips
        // this screen agrees with it. Best effort: a folder a file manager left
        // read-only must not be what stops PLAY from starting the game.
        try {
            if (!versions.isDirectory()) {
                versions.mkdirs();
            }
            write(new File(versions, "version.txt"), new File(resources).getName() + "\n");
        } catch (IOException ignored) {
            // env.txt is the launch. Nothing else is needed for it.
        }
        return resources;
    }

    /** Starts the game in this same task, so BACK comes back here. */
    public void launch(Activity activity) {
        // The mods' Java behaviour starts before the game does, in this process,
        // where the engine's bridge can reach it. One worker; the game launches
        // without waiting, and a slow dex never delays a launch. The mods live
        // in the instance's own folder — the same one the Mods screen reads.
        // Every start comes through here (PLAY, a world, a server), so the
        // runtime is up whichever door the player walked in.
        final File mods = Instances.dir(files, instance());
        new Thread(() -> BehaviorRuntime.loadAll(activity.getApplicationContext(),
                new File(mods, "cryon-mods")), "cryon-behavior").start();
        Intent intent = new Intent();
        intent.setClassName(activity.getPackageName(), GAME_ACTIVITY);
        activity.startActivity(intent);
    }

    /** The newest prepared version, which is what a launch from any other screen runs. */
    public Version chosen() {
        String remembered = versionLabel();
        for (Version version : versions()) {
            if (version.playable && version.label.equals(remembered)) {
                return version;
            }
        }
        for (Version version : versions()) {
            if (version.playable) {
                return version;
            }
        }
        return null;
    }

    /** The version the Versions screen was left on, or "" when nothing was chosen. */
    public String versionLabel() {
        File marker = new File(versions, "version.txt");
        if (!marker.isFile()) {
            return "";
        }
        try {
            return new String(java.nio.file.Files.readAllBytes(marker.toPath()),
                    StandardCharsets.UTF_8).trim();
        } catch (IOException unreadable) {
            return "";
        }
    }

    /**
     * Remember the chosen version.
     *
     * The same file the game's own version choice reads, so a launch that never came
     * through this screen — a resume, a world tapped from the list — agrees with it.
     */
    public void setVersion(String label) throws IOException {
        if (!versions.isDirectory() && !versions.mkdirs()) {
            throw new IOException("could not make " + versions);
        }
        write(new File(versions, "version.txt"), label + "\n");
    }

    /** Where a version's own files live, prepared or not. */
    public File versionDir(String label) {
        return new File(versions, label);
    }

    /** The folder of versions, which is also where Mojang's layout is kept. */
    public File versionsRoot() {
        return versions;
    }

    /**
     * Start the game in one world of the current instance.
     *
     * Same handoff as PLAY, with the name added: the game opens that save instead of
     * the title screen. It goes through the game's own quick-play check, so a world
     * deleted between here and there lands the player on the title screen rather than
     * making a new save under a name they never chose.
     */
    /**
     * Start the game in one world of one instance.
     *
     * The instance is switched first, because a world tapped in another instance's
     * list means "play that one", and the world name goes through the game's own
     * quick-play check: a save deleted between here and there lands the player on
     * the title screen rather than making a new one under a name they never chose.
     */
    public void launchWorld(Activity activity, String instance, String world)
            throws IOException {
        Instances.setCurrent(files, instance);
        start(activity, Collections.singletonList("CRYON_WORLD=" + world));
    }

    /** Start the game joined to a multiplayer address. */
    public void launchServer(Activity activity, String instance, String address)
            throws IOException {
        Instances.setCurrent(files, instance);
        start(activity, Collections.singletonList("CRYON_SERVER=" + address));
    }

    /** The launch every screen shares: the newest prepared version, plus these switches. */
    private void start(Activity activity, List<String> switches) throws IOException {
        Version version = chosen();
        if (version == null) {
            throw new IOException("No prepared version on this device.");
        }
        writeStart(version.dir.getAbsolutePath(), switches);
        launch(activity);
    }

    /**
     * The world the next start will open, if a Worlds screen named one. The list
     * marks it, because a launch already queued is a thing worth seeing.
     */
    public String worldToOpen() {
        String world = readLines(envFile(), '=').get("CRYON_WORLD");
        return world == null || world.isEmpty() ? null : world;
    }

    /**
     * The name the player goes by in a world.
     *
     * The game keeps it in `cryon-player.txt` beside `options.txt`, in the instance,
     * because the name is what decides the offline identity — the same name gets the
     * same inventory on a friend's world. Empty means the game has not been run there
     * yet, and it will choose one for itself on the first launch.
     */
    public String playerName() {
        String name = readLines(new File(gameDir(), "cryon-player.txt"), ':').get("name");
        return name == null ? "" : name;
    }

    /** Vanilla's own rule: 3 to 16 of the characters a name may hold. */
    public static boolean validName(String name) {
        return name != null && name.matches("[A-Za-z0-9_]{3,16}");
    }

    /**
     * Set the name, in the game's own file and format.
     *
     * Written as `name:<one line>` because that is what `offline_name::parse` reads;
     * anything else would leave the game generating a second name behind this one.
     */
    public void setPlayerName(String name) throws IOException {
        if (!validName(name)) {
            throw new IOException("A name is 3 to 16 letters, digits or _");
        }
        write(new File(gameDir(), "cryon-player.txt"), "name:" + name + "\n");
    }

    /** Remove a file, or a folder and everything under it. */
    public static void deleteAll(File file) {
        File[] kids = file.listFiles();
        if (kids != null) {
            for (File kid : kids) {
                deleteAll(kid);
            }
        }
        file.delete();
    }

    static void write(File target, String text) throws IOException {
        File parent = target.getAbsoluteFile().getParentFile();
        if (!parent.isDirectory() && !parent.mkdirs()) {
            throw new IOException("Cannot create " + parent);
        }
        File temporary = File.createTempFile(".cryon-", ".tmp", parent);
        try {
            try (FileOutputStream bytes = new FileOutputStream(temporary);
                    Writer out = new OutputStreamWriter(bytes, StandardCharsets.UTF_8)) {
                out.write(text);
                out.flush();
                bytes.getFD().sync();
            }
            try {
                java.nio.file.Files.move(temporary.toPath(), target.toPath(),
                        java.nio.file.StandardCopyOption.ATOMIC_MOVE,
                        java.nio.file.StandardCopyOption.REPLACE_EXISTING);
            } catch (java.nio.file.AtomicMoveNotSupportedException unavailable) {
                java.nio.file.Files.move(temporary.toPath(), target.toPath(),
                        java.nio.file.StandardCopyOption.REPLACE_EXISTING);
            }
        } finally {
            temporary.delete();
        }
    }

    /**
     * One of these files as an ordered map, so a key the launcher does not know
     * survives a rewrite. `options.txt` holds a player's keybinds and sound levels
     * as well as the settings this app edits, and losing those because a slider was
     * moved would be an unforgivable bug.
     *
     * Comments are dropped: these files belong to the game and this launcher now,
     * and a hand-written note in one of them is a thing the player was told to
     * delete to reset.
     */
    static Map<String, String> readLines(File file, char separator) {
        try {
            return readLinesChecked(file, separator);
        } catch (IOException unreadable) {
            return new LinkedHashMap<>();
        }
    }

    /** Mutations must never replace an unreadable file with defaults. */
    static Map<String, String> readLinesChecked(File file, char separator) throws IOException {
        Map<String, String> lines = new LinkedHashMap<>();
        if (!file.exists()) return lines;
        if (!file.isFile() || file.length() > 1024 * 1024) {
            throw new IOException("Settings file is invalid or too large: " + file.getName());
        }
        for (String line : java.nio.file.Files.readAllLines(file.toPath(), StandardCharsets.UTF_8)) {
            int at = line.indexOf(separator);
            if (at <= 0 || line.trim().startsWith("#")) continue;
            lines.put(line.substring(0, at).trim(), line.substring(at + 1));
        }
        return lines;
    }

    static void writeLines(File target, char separator, Map<String, String> lines)
            throws IOException {
        StringBuilder text = new StringBuilder();
        for (Map.Entry<String, String> line : lines.entrySet()) {
            text.append(line.getKey()).append(separator).append(line.getValue()).append('\n');
        }
        if (target.getParentFile() != null) {
            target.getParentFile().mkdirs();
        }
        write(target, text.toString());
    }
}
