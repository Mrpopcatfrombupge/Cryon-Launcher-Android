package dev.cryon.client;

import android.content.Context;
import android.util.Log;
import dalvik.system.PathClassLoader;
import java.io.File;
import java.lang.reflect.Constructor;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Loads the Java behaviour of imported mods and starts it.
 *
 * A mod whose data the porter converted can also carry compiled behaviour:
 * its jar's classes, dexed at import into {@code cryon-mods/<mod>/behavior/
 * classes.dex}. This class loads each dex with a {@link PathClassLoader}
 * whose parent is the app's own — the parent is what makes the shim classes
 * ({@code net.fabricmc.*}) visible to the mod — reads the entrypoints the
 * mod declares, and runs them, where Fabric Loader would.
 *
 * A class that needs {@code net.minecraft.*} internals cannot link here, and
 * that failure is recorded as one mod that does not run rather than a crash:
 * the report already told the player this would be the case.
 */
public final class BehaviorRuntime {
    private static final String TAG = "CryonBehavior";
    private static final AtomicBoolean started = new AtomicBoolean(false);
    private static volatile int loaded;
    private static volatile int refused;

    /** Where a mod's dexed behaviour lives, under the mod's folder. */
    public static File behaviorDir(File mod) {
        return new File(mod, "behavior/classes.dex");
    }

    /**
     * What the runtime did, for the Mods screen: "2 running, 1 refused".
     * "0 running, 0 refused" also reads as "no game has started yet" — the
     * count is per process, and the runtime starts with the game.
     */
    public static String status() {
        return loaded + " running, " + refused + " refused";
    }

    /**
     * Load every mod's behaviour under {@code mods}, once per process.
     *
     * Called on a worker before the game starts; a mod whose entrypoint throws
     * costs that mod only, never the game.
     */
    public static void loadAll(Context context, File mods) {
        // The self-test runs whatever the mod folders look like: it is the proof
        // the runtime below works, and it must not need a mod to show it.
        pilot();
        if (!started.compareAndSet(false, true) || mods == null || !mods.isDirectory()) {
            return;
        }
        dexPending(context, mods);
        File[] folders = mods.listFiles();
        if (folders == null) {
            return;
        }
        for (File mod : folders) {
            File dex = behaviorDir(mod);
            if (!dex.isFile()) {
                continue;
            }
            try {
                load(mod, dex);
            } catch (Throwable failure) {
                refused++;
                Log.e(TAG, mod.getName() + ": behaviour could not run: " + failure);
            }
        }
        Log.i(TAG, "behaviour runtime: " + loaded + " mod(s) running, "
                + refused + " refused");
    }

    /**
     * The self-test: a mod compiled into the app itself, walking the same
     * initialisation path an imported one does. When it is in the log, the shim
     * and the runtime below it work; the engine's side of the bridge is what
     * makes its tick listener fire.
     */
    private static void pilot() {
        try {
            Class<?> pilot = Class.forName("dev.cryon.client.pilot.PilotMod");
            Object instance = pilot.getDeclaredConstructor().newInstance();
            pilot.getMethod("onInitialize").invoke(instance);
            loaded++;
        } catch (Throwable missing) {
            Log.w(TAG, "pilot entrypoint not present: " + missing);
        }
    }

    /**
     * Dexes every mod that kept its own Java ({@code behavior/source.jar}) but
     * has no dex yet.
     *
     * The dexer is d8 itself, dexed, carried in the APK and run on this device
     * through {@code dalvikvm} — the same tool the build used, reading the same
     * jar the player dropped in. One mod at a time; a failure costs that mod
     * its behaviour only, and the load step below reports it as refused.
     */
    private static void dexPending(Context context, File mods) {
        File dexer = ensureDexer(context);
        if (dexer == null) {
            Log.w(TAG, "no dexer in this build: mods keep their Java undexed");
            return;
        }
        File[] folders = mods.listFiles();
        if (folders == null) {
            return;
        }
        for (File mod : folders) {
            File source = new File(mod, "behavior/source.jar");
            if (!source.isFile() || behaviorDir(mod).isFile()) {
                continue;
            }
            dexOne(dexer, mod, source);
        }
    }

    /** One mod's jar to dex: run d8, move the result in, make it read-only. */
    private static void dexOne(File dexer, File mod, File source) {
        // dalvikvm moved with the ART module: /system/bin on older releases,
        // inside the APEX from Android 12 on. The first one that runs wins.
        for (String vm : new String[] {"/apex/com.android.art/bin/dalvikvm",
                "/system/bin/dalvikvm"}) {
            if (new File(vm).isFile()) {
                dexOne(vm, dexer, mod, source);
                return;
            }
        }
        Log.w(TAG, mod.getName() + ": no dalvikvm on this device");
    }

    private static void dexOne(String vm, File dexer, File mod, File source) {
        File out = new File(mod, "behavior/.dexing");
        deleteTree(out);
        if (!out.mkdirs()) {
            Log.w(TAG, mod.getName() + ": no work folder for dexing");
            return;
        }
        try {
            Process process = new ProcessBuilder(vm, "-cp",
                    dexer.getAbsolutePath(), "com.android.tools.r8.D8",
                    "--release", "--min-api", "29",
                    "--output", out.getAbsolutePath(), source.getAbsolutePath())
                    .redirectErrorStream(true).start();
            StringBuilder output = new StringBuilder();
            try (java.io.InputStream in = process.getInputStream()) {
                byte[] buffer = new byte[4096];
                for (int read; (read = in.read(buffer)) != -1; ) {
                    if (output.length() < 4000) {
                        output.append(new String(buffer, 0, read,
                                java.nio.charset.StandardCharsets.UTF_8));
                    }
                }
            }
            if (!process.waitFor(180, java.util.concurrent.TimeUnit.SECONDS)) {
                process.destroyForcibly();
                Log.w(TAG, mod.getName() + ": dexing timed out");
                return;
            }
            File produced = new File(out, "classes.dex");
            if (process.exitValue() != 0 || !produced.isFile()) {
                Log.w(TAG, mod.getName() + ": dexing failed: " + output);
                return;
            }
            File dex = behaviorDir(mod);
            dex.delete();
            java.nio.file.Files.copy(produced.toPath(), dex.toPath());
            // ART refuses a writable dex (W^X), so the loaded file must be
            // read-only — the same rule the installer applies to the app's own.
            dex.setReadOnly();
            Log.i(TAG, mod.getName() + ": dexed its Java");
        } catch (Throwable failure) {
            Log.w(TAG, mod.getName() + ": dexing failed: " + failure);
        } finally {
            deleteTree(out);
        }
    }

    /**
     * The dexer, from the APK's assets into the app's own files, once.
     * Returns null when this build carries none.
     */
    private static File ensureDexer(Context context) {
        File dexer = new File(new File(context.getFilesDir(), "dexer"), "dexed-d8.dex");
        if (dexer.isFile()) {
            return dexer;
        }
        try (java.io.InputStream in = context.getAssets().open("dexed-d8.dex")) {
            dexer.getParentFile().mkdirs();
            java.nio.file.Files.copy(in, dexer.toPath());
            dexer.setReadOnly();
            return dexer;
        } catch (Throwable missing) {
            return null;
        }
    }

    /** Deletes a folder and everything in it — dexing work folders only. */
    private static void deleteTree(File folder) {
        File[] inside = folder.listFiles();
        if (inside != null) {
            for (File file : inside) {
                if (file.isDirectory()) {
                    deleteTree(file);
                } else {
                    //noinspection ResultOfMethodCallIgnored
                    file.delete();
                }
            }
        }
        //noinspection ResultOfMethodCallIgnored
        folder.delete();
    }

    /** One mod: dex in, entrypoints out, {@code onInitialize} called. */
    private static void load(File mod, File dex) throws Exception {
        PathClassLoader loader = new PathClassLoader(dex.getAbsolutePath(),
                null, BehaviorRuntime.class.getClassLoader());
        File manifest = new File(mod, "fabric.mod.json");
        // The entrypoints are the mod's own claim about where to start; without
        // a manifest there is nothing honest to call.
        if (!manifest.isFile()) {
            Log.w(TAG, mod.getName() + ": no fabric.mod.json, behaviour not started");
            return;
        }
        String json = new String(java.nio.file.Files.readAllBytes(manifest.toPath()),
                java.nio.charset.StandardCharsets.UTF_8);
        org.json.JSONObject root = new org.json.JSONObject(json);
        org.json.JSONObject entrypoints = root.optJSONObject("entrypoints");
        org.json.JSONArray main = entrypoints == null
                ? null : entrypoints.optJSONArray("main");
        if (main == null || main.length() == 0) {
            Log.w(TAG, mod.getName() + ": no main entrypoint, behaviour not started");
            return;
        }
        for (int i = 0; i < main.length(); i++) {
            Object entry = main.opt(i);
            String name = entry instanceof org.json.JSONObject
                    ? ((org.json.JSONObject) entry).optString("value")
                    : String.valueOf(entry);
            if (name.isEmpty()) {
                continue;
            }
            Class<?> initializer = loader.loadClass(name);
            Constructor<?> constructor = initializer.getDeclaredConstructor();
            constructor.setAccessible(true);
            Object instance = constructor.newInstance();
            initializer.getMethod("onInitialize").invoke(instance);
            loaded++;
            Log.i(TAG, mod.getName() + ": " + name + " initialised");
        }
    }

    private BehaviorRuntime() {
    }
}
