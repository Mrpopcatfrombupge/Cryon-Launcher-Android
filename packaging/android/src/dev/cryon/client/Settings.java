package dev.cryon.client;

import java.io.File;
import java.io.IOException;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * The settings, as this launcher is allowed to touch them.
 *
 * Four files, and the launcher has to know which one each setting lives in because
 * they are not the same format:
 *
 *  - `options.txt` in the instance, which is vanilla's: `key:value`, colon, and it
 *    must carry a `version:` line or the engine reads none of it.
 *  - `cryon-graphics.txt` beside it, Cryon's own, also `key:value`, for the
 *    settings vanilla has no word for.
 *  - `env.txt` in the app's files folder, which is `key=value` and is how a phone
 *    passes what would be a command-line switch on a desktop.
 *  - `launcher.txt`, also `key=value`, for the things that are this app's business
 *    and not the game's — how it downloads, what it asks about. Writing those into
 *    the game's files would be a lie about who owns them.
 *
 * The two screens are split the same way: *Game settings* is how the world looks and
 * how it is controlled, and *Launcher settings* is how the thing is run on this
 * phone — the renderer, the resolution, the frame pacing, the cutout, the downloads.
 * A player looking for "which renderer" should not have to guess that it lives
 * beside "clouds".
 *
 * Every range, token and default here is the engine's, read from its parser rather
 * than remembered, because the failure is invisible: a value outside the range is
 * dropped, the default is used, and the player sees a setting that appears to do
 * nothing. That is the bug this class exists to prevent.
 */
public final class Settings {
    /** Where a value lives. */
    private static final int OPTIONS = 0;
    private static final int GRAPHICS = 1;
    private static final int ENV = 2;
    private static final int LAUNCHER = 3;

    /** How the world looks and how it is played. */
    public static final String[] GAME_TABS = {"Video", "Quality", "Controls"};
    /** How this phone runs it. */
    public static final String[] LAUNCHER_TABS = {"Renderer", "Device", "Appearance", "Downloads"};
    public static final String[] TABS = {"Video", "Quality", "Controls", "Renderer",
            "Device", "Appearance", "Downloads"};

    /**
     * One setting. `key` is what the file says, `labels` what the screen says and
     * `tokens` what the file says for each choice — those are not the same thing
     * twice over: particles are a number in the file and words on the screen, a
     * switch is `true` and `false`, and present mode is lower-case in the file.
     */
    public static final class Option {
        public final String key;
        public final String label;
        public final int file;
        public final double from;
        public final double to;
        public final double step;
        public final String[] labels;
        public final String[] tokens;
        /** The stored value is display divided by this, plus that. */
        public final double scale;
        public final double shift;
        public String group = "Video";
        /** What the engine uses when the file says nothing, in the unit shown. */
        public double dflt = Double.NaN;
        /** A line of explanation under the row, for the ones that need it. */
        public String note;

        Option(String key, String label, int file, double from, double to, double step,
               double scale, double shift, String[] labels, String[] tokens) {
            this.key = key;
            this.label = label;
            this.file = file;
            this.from = from;
            this.to = to;
            this.step = step;
            this.scale = scale;
            this.shift = shift;
            this.labels = labels;
            this.tokens = tokens;
        }

        static Option of(String key, String label, int file, String[] names, String[] stored) {
            return new Option(key, label, file, 0, names.length - 1, 1, 1, 0, names, stored);
        }

        static Option toggle(String key, String label, int file) {
            return of(key, label, file, new String[] {"Off", "On"},
                    new String[] {"false", "true"});
        }

        static Option slider(String key, String label, int file, double from, double to,
                             double step) {
            return new Option(key, label, file, from, to, step, 1, 0, null, null);
        }

        /** A slider whose file value is a transform of the number on screen. */
        static Option slider(String key, String label, int file, double from, double to,
                             double step, double scale, double shift) {
            return new Option(key, label, file, from, to, step, scale, shift, null, null);
        }

        Option in(String which) {
            group = which;
            return this;
        }

        Option at(double defaultDisplay) {
            dflt = defaultDisplay;
            return this;
        }

        Option says(String text) {
            note = text;
            return this;
        }

        boolean isChoice() {
            return labels != null;
        }

        double display(double stored) {
            return stored * scale + shift;
        }

        double stored(double display) {
            return (display - shift) / scale;
        }

        /** The value as it goes into the file, in the engine's own form. */
        double normalize(double display) {
            if (!Double.isFinite(display)) {
                display = Double.isNaN(dflt) ? from : dflt;
            }
            double bounded = Math.max(from, Math.min(to, display));
            return Math.max(from, Math.min(to, from + Math.round((bounded - from) / step) * step));
        }

        String encode(double display) {
            double value = stored(normalize(display));
            if (isChoice()) {
                int at = (int) Math.round(value);
                if (at < 0) {
                    at = 0;
                }
                if (at >= tokens.length) {
                    at = tokens.length - 1;
                }
                return tokens[at];
            }
            // Whole steps of a whole range are written as an integer: the engine
            // reads `renderDistance:12`, and a `12.0` in a file vanilla also writes
            // is a thing no other tool will produce.
            boolean integral = scale == 1 && shift == 0
                    && step == Math.floor(step) && from == Math.floor(from);
            return integral ? Integer.toString((int) Math.round(value))
                            : Double.toString(value);
        }

        double decode(String text) {
            String trimmed = text.trim();
            if (isChoice()) {
                for (int i = 0; i < tokens.length; i++) {
                    if (tokens[i].equalsIgnoreCase(trimmed)) {
                        return i;
                    }
                }
                String bare = trimmed.replace("\"", "");
                for (int i = 0; i < labels.length; i++) {
                    if (labels[i].equalsIgnoreCase(bare)) {
                        return i;
                    }
                }
                return stored(Double.isNaN(dflt) ? from : dflt);
            }
            try {
                double parsed = Double.parseDouble(trimmed);
                return Double.isFinite(parsed) ? parsed : stored(Double.isNaN(dflt) ? from : dflt);
            } catch (NumberFormatException absent) {
                return stored(Double.isNaN(dflt) ? from : dflt);
            }
        }
    }

    /**
     * What the screen offers. Ranges, tokens and defaults are the engine's: its
     * options table (crates/cryon-client-render/src/options_file.rs), its own
     * graphics parser (graphics.rs:499-553) and its `Default` (settings.rs:208),
     * with the Android defaults at graphics.rs:458 read as the shipped values.
     */
    public static final Option[] ALL = {
        // --- Video ---
        Option.slider("renderDistance", "Render distance", OPTIONS, 2, 32, 1)
                .in("Video").at(12),
        Option.slider("simulationDistance", "Simulation distance", OPTIONS, 5, 32, 1)
                .in("Video").at(12),
        Option.slider("resolutionScale", "Resolution scale", GRAPHICS, 50, 200, 5)
                .in("Renderer").at(75)
                .says("Below 100% the world is drawn smaller and stretched up. "
                        + "The biggest win on a phone, and the first thing to lower "
                        + "when a high render distance stutters."),
        Option.slider("maxFps", "Frame limit", OPTIONS, 10, 260, 10)
                .in("Video").at(120),
        Option.slider("guiScale", "Interface scale", OPTIONS, 0, 4, 1)
                .in("Video").at(0),
        Option.toggle("enableVsync", "VSync", OPTIONS).in("Video").at(1),
        Option.of("presentMode", "Present mode", GRAPHICS,
                new String[] {"Auto", "Fifo", "Mailbox", "Immediate"},
                new String[] {"auto", "fifo", "mailbox", "immediate"})
                .in("Renderer").at(0)
                .says("Mailbox presents as soon as a frame is ready without tearing; "
                        + "it only applies with VSync off."),
        Option.of("framePacing", "Frame pacing", GRAPHICS,
                new String[] {"Efficient", "Precise"},
                new String[] {"efficient", "precise"})
                .in("Renderer").at(0),
        Option.of("overlay", "Frame readout", GRAPHICS,
                new String[] {"Off", "FPS", "Graph"},
                new String[] {"off", "fps", "graph"}).in("Renderer").at(0),
        Option.toggle("batteryMode", "Battery mode", GRAPHICS).in("Renderer").at(0),

        // --- Quality ---
        Option.of("particles", "Particles", OPTIONS,
                new String[] {"All", "Decreased", "Minimal"},
                new String[] {"0", "1", "2"}).in("Quality").at(0),
        Option.of("renderClouds", "Clouds", OPTIONS,
                new String[] {"Off", "Fast", "Fancy"},
                new String[] {"\"false\"", "\"fast\"", "\"true\""}).in("Quality").at(2),
        Option.slider("mipmapLevels", "Mipmaps", OPTIONS, 0, 4, 1).in("Quality").at(4),
        Option.slider("entityDistanceScaling", "Entity distance", OPTIONS, 0.5, 5, 0.25)
                .in("Quality").at(1.0),
        Option.toggle("ao", "Smooth lighting", OPTIONS).in("Quality").at(1),
        Option.toggle("entityShadows", "Entity shadows", OPTIONS).in("Quality").at(1),
        Option.toggle("vignette", "Vignette", OPTIONS).in("Quality").at(1),
        Option.of("farDetail", "Far detail", GRAPHICS,
                new String[] {"Full", "Reduced"},
                new String[] {"full", "reduced"}).in("Renderer").at(1)
                .says("Reduced starts the coarse terrain levels closer in. "
                        + "It changes the picture at distance and buys frames."),
        Option.toggle("sky", "Sky", GRAPHICS).in("Quality").at(1),
        Option.toggle("fog", "Fog", GRAPHICS).in("Quality").at(1),
        Option.toggle("weather", "Weather", GRAPHICS).in("Quality").at(1),
        Option.toggle("animatedTextures", "Animated textures", GRAPHICS).in("Renderer").at(0)
                .says("Off by default on a phone: water, fire and portals update "
                        + "every frame otherwise."),
        Option.toggle("caveCulling", "Cull unseen chunks", GRAPHICS).in("Renderer").at(1),
        Option.toggle("entityCulling", "Cull hidden entities", GRAPHICS).in("Renderer").at(1),
        Option.toggle("fxaa", "FXAA", GRAPHICS).in("Renderer").at(0),
        Option.toggle("taa", "TAA", GRAPHICS).in("Renderer").at(0)
                .says("Unavailable on the OpenGL renderer, which cannot read depth "
                        + "textures the way this pass does."),
        Option.of("upscaling", "Upscaling", GRAPHICS,
                new String[] {"Native", "Quality", "Balanced", "Performance",
                        "Ultra performance"},
                new String[] {"native", "quality", "balanced", "performance",
                        "ultraperformance"}).in("Renderer").at(0),

        // --- Controls ---
        Option.slider("fov", "Field of view", OPTIONS, 30, 110, 1, 40, 70)
                .in("Controls").at(70),
        Option.slider("mouseSensitivity", "Sensitivity", OPTIONS, 0, 1, 0.01)
                .in("Controls").at(0.5),
        Option.toggle("invertYMouse", "Invert Y", OPTIONS).in("Controls").at(0),
        Option.toggle("autoJump", "Auto jump", OPTIONS).in("Controls").at(0),

        // --- Device ---
        Option.of("graphicsApi", "Renderer", GRAPHICS,
                new String[] {"Auto", "Vulkan", "OpenGL ES"},
                new String[] {"auto", "vulkan", "opengl"}).in("Renderer").at(0)
                .says("Restart the game to apply. Auto tries Vulkan then OpenGL ES. "
                        + "Driver support and performance vary; OpenGL ES does not "
                        + "support TAA or temporal upscaling."),
        Option.of("CRYON_CUTOUT", "Front camera", ENV,
                new String[] {"Draw under it", "Stop before it"},
                new String[] {"fill", "inset"}).in("Device").at(0)
                .says("Drawing under the camera fills the whole screen; stopping "
                        + "before it leaves a black band where the cutout is."),
        Option.of("CRYON_AUDIO", "Sound", ENV,
                new String[] {"Off", "On"},
                new String[] {"0", "1"}).in("Device").at(0)
                .says("Sound is off by default on a phone: the mixer shares one lock "
                        + "with the audio callback, which costs more than a frame."),

        // --- Engine ---
        Option.slider("chunkUpdates", "Chunk updates per frame", GRAPHICS, 1, 64, 1)
                .in("Renderer").at(8)
                .says("How much meshing work is done before the engine yields to the "
                        + "frame. Lower is smoother while chunks stream in."),
        Option.slider("uploadBudgetKiB", "Upload budget", GRAPHICS, 256, 32768, 256)
                .in("Renderer").at(4096).says("Kibibytes of terrain mesh data uploaded "
                        + "per frame. A single larger mesh is allowed to avoid starvation."),
        Option.slider("workers", "Worker threads", GRAPHICS, 0, 16, 1)
                .in("Renderer").at(0).says("Auto leaves a core for the game. "
                        + "Battery mode reduces the automatic count. Restart to apply."),
        Option.of("CRYON_GPU_CULL", "GPU occlusion culling", ENV,
                new String[] {"Off", "On"},
                new String[] {"0", "1"}).in("Renderer").at(1),
        Option.of("CRYON_FLAT_DRAWS", "Flat draw batching", ENV,
                new String[] {"Off", "On"},
                new String[] {"0", "1"}).in("Renderer").at(1),

        // --- Appearance: launcher-only, never sent to the game ---
        Option.of("launcherMotion", "Background motion", LAUNCHER,
                new String[] {"Reduced", "Animated"}, new String[] {"0", "1"})
                .in("Appearance").at(0)
                .says("Reduced motion uses no repeating animation frames. "
                        + "Animations always stop behind the game."),
        Option.of("launcherAccent", "Accent color", LAUNCHER,
                new String[] {"Ice", "Mint", "Violet"},
                new String[] {"ice", "mint", "violet"}).in("Appearance").at(0),
        Option.of("launcherCompact", "Home layout", LAUNCHER,
                new String[] {"Comfortable", "Compact"}, new String[] {"0", "1"})
                .in("Appearance").at(0),
        Option.slider("launcherTextScale", "Launcher text size", LAUNCHER, 85, 125, 5)
                .in("Appearance").at(100),
        Option.slider("gamma", "Brightness", OPTIONS, 0, 1, 0.05).in("Video").at(0.5),
        Option.toggle("bobView", "View bobbing", OPTIONS).in("Controls").at(1),
        Option.toggle("showSubtitles", "Subtitles", OPTIONS).in("Controls").at(0),

        // --- Downloads ---
        Option.of("CRYON_METERED", "On a metered network", LAUNCHER,
                new String[] {"Ask first", "Download anyway", "Never download"},
                new String[] {"ask", "allow", "refuse"}).in("Downloads").at(0)
                .says("A version is about 600 MB, and most of that is Mojang's sounds "
                        + "and textures. Asking costs nothing; a carrier bill does."),

        // --- Controls ---
        Option.slider("CRYON_BUTTON_SCALE", "Button size", ENV, 50, 200, 5)
                .in("Controls").at(100)
                .says("Every control grows and shrinks together. 100 is the size the "
                        + "layout was saved at."),
        Option.slider("CRYON_LONG_PRESS", "Hold to dig", ENV, 100, 1000, 10)
                .in("Controls").at(300)
                .says("How long a finger has to rest on the glass before it counts as "
                        + "the left button, in milliseconds."),
        Option.of("CRYON_GESTURES", "Gestures on the glass", ENV,
                new String[] {"Off", "On"}, new String[] {"0", "1"})
                .in("Controls").at(1)
                .says("Off leaves the glass for looking around only, and every dig and "
                        + "place has to come from a button."),
    };

    /** The current value of one setting, in the unit the screen shows. */
    public static double value(Cryon cryon, Option option) {
        String text = Cryon.readLines(target(cryon, option), separator(option)).get(option.key);
        if (text == null) {
            // No line yet: the game uses its default, so that is what the screen
            // shows. A slider parked at the bottom of its range would claim the
            // player had set it there.
            return Double.isNaN(option.dflt) ? option.from : option.dflt;
        }
        return option.normalize(option.display(option.decode(text)));
    }

    /** Read each settings file once, not once per row on the UI thread. */
    public static Map<String, Double> snapshot(Cryon cryon) {
        Map<Integer, Map<String, String>> files = new LinkedHashMap<>();
        Map<String, Double> values = new LinkedHashMap<>();
        for (Option option : ALL) {
            Map<String, String> lines = files.get(option.file);
            if (lines == null) {
                lines = Cryon.readLines(target(cryon, option), separator(option));
                files.put(option.file, lines);
            }
            String text = lines.get(option.key);
            double value = text == null ? (Double.isNaN(option.dflt) ? option.from : option.dflt)
                    : option.display(option.decode(text));
            values.put(option.key, option.normalize(value));
        }
        return values;
    }

    public static boolean supported(Map<String, Double> values, Option option) {
        boolean gles = values.getOrDefault("graphicsApi", 0.0) == 2.0;
        return !gles || !(option.key.equals("taa") || option.key.equals("upscaling")
                || option.key.equals("CRYON_GPU_CULL"));
    }

    public static boolean supported(Cryon cryon, Option option) {
        return supported(snapshot(cryon), option);
    }

    /** Bounded presets change only graphics quality, never backend, account or controls. */
    public static void applyPreset(Cryon cryon, String name) throws IOException {
        boolean mobile = "mobile".equalsIgnoreCase(name);
        boolean quality = "quality".equalsIgnoreCase(name);
        if (!mobile && !quality && !"balanced".equalsIgnoreCase(name)) {
            throw new IOException("Unknown preset: " + name);
        }
        Map<String, Double> values = new LinkedHashMap<>();
        values.put("renderDistance", mobile ? 6.0 : quality ? 16.0 : 10.0);
        values.put("simulationDistance", mobile ? 5.0 : quality ? 10.0 : 6.0);
        values.put("resolutionScale", mobile ? 75.0 : 100.0);
        values.put("maxFps", mobile ? 60.0 : quality ? 120.0 : 60.0);
        values.put("enableVsync", 1.0);
        values.put("particles", mobile ? 2.0 : quality ? 0.0 : 1.0);
        values.put("renderClouds", mobile ? 0.0 : quality ? 2.0 : 1.0);
        values.put("mipmapLevels", quality ? 4.0 : 2.0);
        values.put("entityDistanceScaling", mobile ? 0.5 : quality ? 1.0 : 0.75);
        values.put("entityShadows", mobile ? 0.0 : 1.0);
        values.put("ao", mobile ? 0.0 : 1.0);
        values.put("chunkUpdates", mobile ? 4.0 : quality ? 12.0 : 8.0);
        values.put("uploadBudgetKiB", mobile ? 2048.0 : quality ? 8192.0 : 4096.0);
        values.put("animatedTextures", quality ? 1.0 : 0.0);
        values.put("farDetail", quality ? 0.0 : 1.0);
        values.put("framePacing", 0.0);
        Map<Integer, Map<String, String>> changed = new LinkedHashMap<>();
        Map<Integer, Option> owners = new LinkedHashMap<>();
        for (Option option : ALL) {
            if (!values.containsKey(option.key)) continue;
            Map<String, String> lines = changed.get(option.file);
            if (lines == null) {
                lines = Cryon.readLinesChecked(target(cryon, option), separator(option));
                changed.put(option.file, lines);
                owners.put(option.file, option);
            }
            lines.put(option.key, option.encode(values.get(option.key)));
            if (option.file == OPTIONS) lines.putIfAbsent("version", "5023");
        }
        for (Map.Entry<Integer, Map<String, String>> file : changed.entrySet()) {
            Option owner = owners.get(file.getKey());
            Cryon.writeLines(target(cryon, owner), separator(owner), file.getValue());
        }
    }

    /**
     * Write one setting and leave the rest of the file alone.
     *
     * The whole file is rewritten because a player's keybinds, sound levels and
     * chat settings live in it too, and they are kept: only the line for this key
     * changes, and a key the launcher does not know is written back untouched. The
     * version line is re-added to options.txt if it is missing, since without it
     * the engine reads none of the file.
     */
    public static void put(Cryon cryon, Option option, double display) throws IOException {
        File file = target(cryon, option);
        Map<String, String> lines = Cryon.readLinesChecked(file, separator(option));
        lines.put(option.key, option.encode(display));
        if (option.file == OPTIONS && !lines.containsKey("version")) {
            // Ordered first, the way the game writes it.
            Map<String, String> withVersion = new LinkedHashMap<>();
            withVersion.put("version", "5023");
            withVersion.putAll(lines);
            lines = withVersion;
        }
        Cryon.writeLines(file, separator(option), lines);
    }

    /** Which file a setting lives in. */
    private static File target(Cryon cryon, Option option) {
        switch (option.file) {
            case GRAPHICS:
                return new File(cryon.gameDir(), "cryon-graphics.txt");
            case ENV:
                return cryon.envFile();
            case LAUNCHER:
                return cryon.launcherFile();
            default:
                return new File(cryon.gameDir(), "options.txt");
        }
    }

    /** Colon in the game's files, equals in the switch files. */
    private static char separator(Option option) {
        return option.file == ENV || option.file == LAUNCHER ? '=' : ':';
    }

    private Settings() {
    }
}
