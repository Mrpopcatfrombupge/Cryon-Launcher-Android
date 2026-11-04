package dev.cryon.client;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import org.json.JSONArray;
import org.json.JSONObject;

/**
 * Mojang's own download endpoints, used the way every other launcher uses them.
 *
 * The version list, each version's file, the client jar and the assets all come from
 * Mojang's servers, and every file is checked against the sha1 and size Mojang gives
 * for it before it is kept — which is the only reason a downloaded file can be
 * trusted to be the game rather than something that merely has the game's name.
 *
 * Nothing here is a mirror: these are the same addresses the official launcher reads.
 * Nor is anything bundled: a version appears on a device only because somebody asked
 * for it there. Files already present and whole are kept, so a download that was
 * stopped — by a cancel, a dead connection or a battery — is finished by the next one
 * instead of started again.
 */
public final class Mojang {
    /** Mojang's list of every version they publish, with the sha1 of each version file. */
    public static final String MANIFEST =
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
    /** Where an asset object lives, by the first two characters of its sha1. */
    private static final String OBJECTS = "https://resources.download.minecraft.net";
    /** How many files to fetch at once: enough to fill a phone's link, few enough to keep it. */
    private static final int THREADS = 4;

    /** One version as the list describes it. */
    public static final class Release {
        public final String id;
        /** `release`, `snapshot`, `old_beta`, `old_alpha`. */
        public final String type;
        public final String url;
        public final String sha1;
        /** When it came out, as Mojang writes it: `2026-06-10T09:00:00+00:00`. */
        public final String released;

        Release(JSONObject json) {
            id = json.optString("id");
            type = json.optString("type");
            url = json.optString("url");
            sha1 = json.optString("sha1");
            released = json.optString("releaseTime");
        }

        /** A list row's date: the day, not the moment. */
        public String day() {
            return released.length() >= 10 ? released.substring(0, 10) : released;
        }
    }

    /** One file to fetch: where from, what it must hash to, and where it goes. */
    public static final class Item {
        public final String url;
        public final String sha1;
        public final long size;
        public final File target;

        Item(String url, String sha1, long size, File target) {
            this.url = url;
            this.sha1 = sha1;
            this.size = size;
            this.target = target;
        }

        /** Whether this file is already here and whole, which is the resume. */
        boolean complete() {
            return target.isFile() && (size <= 0 || target.length() == size)
                    && (!sha1.isEmpty() ? hash(target).equals(sha1) : true);
        }
    }

    /** How far a download is, for a button to show.
     *
     *  The counters can be the caller's own, so a job whose screen polls them sees
     *  the workers' progress while it happens rather than after a stage ends. */
    public static final class Progress {
        public final AtomicLong files;
        public final AtomicLong bytes;
        public long fileCount;
        public long byteCount;

        public Progress() {
            this(new AtomicLong(), new AtomicLong());
        }

        public Progress(AtomicLong files, AtomicLong bytes) {
            this.files = files;
            this.bytes = bytes;
        }
    }

    /**
     * Parse a JSON answer, or say what went wrong in a sentence a player can act on.
     *
     * Android's JSON parser reports a malformed document with a checked exception, and
     * every place that reads Mojang's files would otherwise have to pass it along; here
     * it becomes the thing it actually is, which is a bad answer from a server.
     */
    private static JSONObject object(byte[] body) throws IOException {
        try {
            return new JSONObject(new String(body, "UTF-8"));
        } catch (org.json.JSONException malformed) {
            throw new IOException("Mojang sent something that is not the file it "
                    + "promised: " + malformed.getMessage());
        }
    }

    /** A child object that must be there, or a message saying which file is short. */
    private static JSONObject need(JSONObject parent, String key) throws IOException {
        JSONObject child = parent.optJSONObject(key);
        if (child == null) {
            throw new IOException("the version file has no " + key);
        }
        return child;
    }

    /** Every version Mojang lists, newest first, as their list gives them. */
    public static List<Release> releases() throws IOException {
        return parse(get(MANIFEST, null));
    }

    /**
     * A version list read from Mojang's servers.
     *
     * Kept apart from the fetch so a screen can show the list it saved the last time
     * there was a connection, and say plainly that it is a copy, rather than showing
     * nothing because the network is not there now.
     */
    public static List<Release> parse(byte[] body) throws IOException {
        JSONObject root = object(body);
        JSONArray versions = root.optJSONArray("versions");
        List<Release> found = new ArrayList<>();
        if (versions == null) {
            throw new IOException("Mojang's version list came back without any versions");
        }
        for (int i = 0; i < versions.length(); i++) {
            Release release = new Release(versions.optJSONObject(i));
            if (!release.id.isEmpty() && !release.url.isEmpty()) {
                found.add(release);
            }
        }
        return found;
    }

    /**
     * The version's own file, checked against the sha1 its entry in the list gave.
     *
     * The list is what makes this safe: Mojang's `version_manifest` names each version
     * and hashes the file that describes it, so a version file that was not the one the
     * list promised is refused here rather than downloaded into the game.
     */
    public static JSONObject versionFile(Release release) throws IOException {
        byte[] body = get(release.url, null);
        if (!release.sha1.isEmpty() && !sha1(body).equals(release.sha1)) {
            throw new IOException(release.id + " did not match the hash Mojang's list gave");
        }
        return object(body);
    }

    /**
     * The files a version is made of, in the official launcher's layout under `dir`.
     *
     * The client jar always; the libraries and the logging setup only for a Java run,
     * because the Rust game reads the jar's data through the prepared folder instead of
     * loading classes at run time. Assets come last and are the bulk of it.
     */
    public static List<Item> plan(Release release, File dir, boolean java)
            throws IOException {
        JSONObject json = versionFile(release);
        List<Item> files = new ArrayList<>();
        File versions = new File(new File(dir, "versions"), release.id);
        files.add(item(need(need(json, "downloads"), "client"),
                new File(versions, release.id + ".jar")));
        if (java) {
            JSONArray libraries = json.optJSONArray("libraries");
            if (libraries != null) {
                for (int i = 0; i < libraries.length(); i++) {
                    files.addAll(library(libraries.optJSONObject(i), dir));
                }
            }
            JSONObject logging = json.optJSONObject("logging");
            if (logging != null) {
                JSONObject client = logging.optJSONObject("client");
                if (client != null && client.optJSONObject("file") != null) {
                    JSONObject file = need(client, "file");
                    files.add(item(file,
                            new File(new File(dir, "assets/log_configs"),
                                    file.optString("id"))));
                }
            }
        }
        JSONObject index = json.optJSONObject("assetIndex");
        if (index != null) {
            files.add(item(index, new File(new File(dir, "assets/indexes"),
                    index.optString("id") + ".json")));
        }
        return files;
    }

    /** One library's jar, and its natives when the version is old enough to split them. */
    private static List<Item> library(JSONObject library, File dir) {
        List<Item> found = new ArrayList<>();
        if (library == null || !runsHere(library)) {
            return found;
        }
        JSONObject downloads = library.optJSONObject("downloads");
        if (downloads == null) {
            return found;
        }
        JSONObject artifact = downloads.optJSONObject("artifact");
        if (artifact != null) {
            found.add(item(artifact, new File(new File(dir, "libraries"),
                    artifact.optString("path"))));
        }
        JSONObject natives = library.optJSONObject("natives");
        if (natives != null) {
            // Before 1.19 a library's natives for this system were a classifier of their own.
            String key = "android";
            JSONObject classifier = downloads.optJSONObject("classifiers");
            JSONObject bundled = classifier == null ? null : classifier.optJSONObject(
                    natives.optString(key, "").replace("${arch}", "64"));
            if (bundled != null) {
                found.add(item(bundled, new File(new File(dir, "natives"),
                        bundled.optString("path"))));
            }
        }
        return found;
    }

    /** Whether a library applies to this device; Mojang's rules say which ones do. */
    private static boolean runsHere(JSONObject library) {
        JSONArray rules = library.optJSONArray("rules");
        if (rules == null) {
            return true;
        }
        boolean allowed = false;
        for (int i = 0; i < rules.length(); i++) {
            JSONObject rule = rules.optJSONObject(i);
            JSONObject os = rule == null ? null : rule.optJSONObject("os");
            boolean ours = os == null || os.optString("name", "android").equals("android");
            if (!ours && rule.optString("action").equals("allow")) {
                continue;
            }
            allowed = rule.optString("action").equals("allow");
        }
        return allowed;
    }

    /**
     * Fetch every file that is not already whole, `THREADS` at a time.
     *
     * Returns the files that could not be had rather than failing the lot: a missing
     * sound is a quieter game, and the next download takes it, while a missing jar is
     * reported by the caller checking whether the version is runnable at all.
     */
    public static List<Item> download(List<Item> files, Progress progress,
            AtomicBoolean cancel) {
        List<Item> missing = new ArrayList<>();
        for (Item item : files) {
            if (!item.complete()) {
                missing.add(item);
            } else if (item.size > 0) {
                // Already here: counted, so a download that resumes shows how much
                // was never needed again rather than starting the bar from nothing.
                progress.bytes.addAndGet(item.size);
            }
        }
        progress.fileCount = missing.size();
        if (missing.isEmpty()) {
            return missing;
        }
        ExecutorService pool = Executors.newFixedThreadPool(THREADS);
        List<Item> failed = new ArrayList<>();
        for (Item item : missing) {
            pool.submit(() -> {
                if (cancel.get()) {
                    return;
                }
                try {
                    fetch(item, progress, cancel);
                } catch (IOException refused) {
                    synchronized (failed) {
                        failed.add(item);
                    }
                }
                progress.files.incrementAndGet();
            });
        }
        pool.shutdown();
        try {
            while (!pool.awaitTermination(200, TimeUnit.MILLISECONDS)) {
                if (cancel.get()) {
                    pool.shutdownNow();
                }
            }
        } catch (InterruptedException stopped) {
            pool.shutdownNow();
            Thread.currentThread().interrupt();
        }
        return failed;
    }

    /**
     * One file, written whole or not at all, streamed straight to disk.
     *
     * A half-written file is worse than a missing one: it has the right name and the
     * wrong bytes, and its size alone can look correct. So this writes to `.part`,
     * checks the hash of what arrived, and only then puts it where the game will read
     * it.
     *
     * The body streams through a small buffer instead of arriving whole in memory:
     * the client jar is hundreds of megabytes, and a launcher that holds one of those
     * in the heap garbage-collects the rest of the app into a stutter while it
     * downloads. The bytes are counted as they land, so the button moves during one
     * big file instead of waiting for it to finish; a retry gives back what the
     * failed attempt counted, so the number never runs ahead of the truth.
     */
    private static void fetch(Item item, Progress progress, AtomicBoolean cancel)
            throws IOException {
        IOException last = null;
        for (int attempt = 1; attempt <= 3; attempt++) {
            if (cancel != null && cancel.get()) {
                throw new IOException("cancelled");
            }
            long counted = 0;
            try {
                HttpURLConnection connection = open(item.url);
                try {
                    int code = connection.getResponseCode();
                    if (code / 100 != 2) {
                        throw new IOException(item.target.getName() + " answered " + code);
                    }
                    MessageDigest digest;
                    try {
                        digest = MessageDigest.getInstance("SHA-1");
                    } catch (Exception impossible) {
                        throw new IOException("no SHA-1 on this device");
                    }
                    File parent = item.target.getParentFile();
                    if (parent != null && !parent.isDirectory() && !parent.mkdirs()) {
                        throw new IOException("could not make " + parent);
                    }
                    File part = new File(item.target.getAbsolutePath() + ".part");
                    try (InputStream in = connection.getInputStream();
                            OutputStream out = new FileOutputStream(part)) {
                        byte[] buffer = new byte[1 << 16];
                        for (int read = in.read(buffer); read > 0; read = in.read(buffer)) {
                            if (cancel != null && cancel.get()) {
                                throw new IOException("cancelled");
                            }
                            digest.update(buffer, 0, read);
                            out.write(buffer, 0, read);
                            counted += read;
                            progress.bytes.addAndGet(read);
                        }
                    }
                    if (!item.sha1.isEmpty() && !hex(digest.digest()).equals(item.sha1)) {
                        throw new IOException(item.target.getName()
                                + " did not hash to what Mojang said");
                    }
                    if (!part.renameTo(item.target)) {
                        throw new IOException("could not put "
                                + item.target.getName() + " in place");
                    }
                    return;
                } finally {
                    connection.disconnect();
                }
            } catch (IOException failed) {
                progress.bytes.addAndGet(-counted);
                last = failed;
                if (attempt < 3) {
                    try {
                        Thread.sleep(500L * attempt);
                    } catch (InterruptedException stopped) {
                        Thread.currentThread().interrupt();
                        throw failed;
                    }
                }
            }
        }
        throw last;
    }

    /**
     * The asset objects one index lists, in Mojang's own layout under `assetsDir`.
     *
     * They can only be named after the index has been downloaded, which is why a
     * download has two stages rather than one plan: the index is small and says what
     * the fifteen thousand files after it are.
     */
    public static List<Item> objects(File assetsDir, File indexFile) throws IOException {
        JSONObject index = object(slurp(indexFile));
        JSONObject listed = index.optJSONObject("objects");
        List<Item> found = new ArrayList<>();
        if (listed == null) {
            return found;
        }
        for (Iterator<String> keys = listed.keys(); keys.hasNext();) {
            String path = keys.next();
            JSONObject object = listed.optJSONObject(path);
            if (object == null) {
                continue;
            }
            String hash = object.optString("hash");
            if (hash.length() < 4) {
                continue;
            }
            found.add(new Item(OBJECTS + "/" + hash.substring(0, 2) + "/" + hash, hash,
                    object.optLong("size", -1), object(assetsDir, hash)));
        }
        return found;
    }

    /**
     * Turn the asset index into the tree the game reads: `objects/ab/hash` becomes
     * `minecraft/textures/block/stone.png`, which is what the index's own keys say.
     *
     * This is the part of preparation that is only file work, so a phone can do it:
     * no Java, no decompiling, just fifteen thousand copies that the index describes.
     */
    public static void virtualise(File indexFile, File into, Progress progress,
            AtomicBoolean cancel) throws IOException {
        JSONObject index = object(slurp(indexFile));
        JSONObject objects = index.optJSONObject("objects");
        if (objects == null) {
            throw new IOException("the asset index lists no objects");
        }
        List<String> paths = new ArrayList<>();
        for (Iterator<String> keys = objects.keys(); keys.hasNext();) {
            paths.add(keys.next());
        }
        progress.fileCount = paths.size();
        progress.files.set(0);
        for (String path : paths) {
            if (cancel.get()) {
                return;
            }
            JSONObject object = objects.optJSONObject(path);
            if (object == null) {
                continue;
            }
            String hash = object.optString("hash");
            File source = object(indexFile.getParentFile().getParentFile(), hash);
            File target = slash(into, path);
            if (!source.isFile()) {
                continue;
            }
            if (target.isFile() && target.length() == object.optLong("size", target.length())) {
                progress.files.incrementAndGet();
                continue;
            }
            copy(source, target);
            progress.files.incrementAndGet();
        }
    }

    /** Where Mojang keeps one object, by the first two characters of its sha1. */
    public static File object(File assetsDir, String hash) {
        return new File(new File(new File(assetsDir, "objects"), hash.substring(0, 2)), hash);
    }

    private static File slash(File root, String path) {
        File file = root;
        for (String part : path.split("/")) {
            file = new File(file, part);
        }
        return file;
    }


    /** A download's whole body, in memory: these are jars and JSON, not the assets. */
    public static byte[] get(String url) throws IOException {
        return get(url, null);
    }

    /** The same, giving up between attempts when the caller asks the download to stop. */
    static byte[] get(String url, AtomicBoolean cancel) throws IOException {
        for (int attempt = 1; attempt <= 3; attempt++) {
            if (cancel != null && cancel.get()) {
                throw new IOException("cancelled");
            }
            try {
                return read(url, cancel);
            } catch (IOException failed) {
                if (attempt == 3) {
                    throw failed;
                }
                try {
                    Thread.sleep(500L * attempt);
                } catch (InterruptedException stopped) {
                    Thread.currentThread().interrupt();
                    throw failed;
                }
            }
        }
        throw new IOException("unreachable");
    }

    /** A connection with the timeouts and name every request here shares. */
    private static HttpURLConnection open(String url) throws IOException {
        HttpURLConnection connection = (HttpURLConnection) new URL(url).openConnection();
        connection.setConnectTimeout(15_000);
        connection.setReadTimeout(30_000);
        connection.setInstanceFollowRedirects(true);
        connection.setRequestProperty("User-Agent", "Cryon");
        return connection;
    }

    private static byte[] read(String url, AtomicBoolean cancel) throws IOException {
        HttpURLConnection connection = open(url);
        try {
            int code = connection.getResponseCode();
            if (code / 100 != 2) {
                throw new IOException(url + " answered " + code);
            }
            try (InputStream in = connection.getInputStream()) {
                java.io.ByteArrayOutputStream body = new java.io.ByteArrayOutputStream(
                        Math.max(1 << 16, connection.getContentLength()));
                byte[] buffer = new byte[1 << 16];
                for (int read = in.read(buffer); read > 0; read = in.read(buffer)) {
                    if (cancel != null && cancel.get()) {
                        throw new IOException("cancelled");
                    }
                    body.write(buffer, 0, read);
                }
                return body.toByteArray();
            }
        } finally {
            connection.disconnect();
        }
    }

    private static byte[] slurp(File file) throws IOException {
        try (InputStream in = new java.io.FileInputStream(file)) {
            java.io.ByteArrayOutputStream body = new java.io.ByteArrayOutputStream(
                    (int) Math.min(file.length(), 1 << 24));
            byte[] buffer = new byte[1 << 16];
            for (int read = in.read(buffer); read > 0; read = in.read(buffer)) {
                body.write(buffer, 0, read);
            }
            return body.toByteArray();
        }
    }

    private static void copy(File from, File to) throws IOException {
        File parent = to.getParentFile();
        if (parent != null && !parent.isDirectory() && !parent.mkdirs()) {
            throw new IOException("could not make " + parent);
        }
        try (InputStream in = new java.io.FileInputStream(from);
                OutputStream out = new FileOutputStream(to)) {
            byte[] buffer = new byte[1 << 16];
            for (int read = in.read(buffer); read > 0; read = in.read(buffer)) {
                out.write(buffer, 0, read);
            }
        }
    }

    private static Item item(JSONObject download, File target) {
        return new Item(download.optString("url"), download.optString("sha1"),
                download.optLong("size", -1), target);
    }

    /** The sha1 of a file already on disk, read a piece at a time, or "" if unreadable. */
    public static String hash(File file) {
        MessageDigest digest;
        try {
            digest = MessageDigest.getInstance("SHA-1");
        } catch (Exception impossible) {
            return "";
        }
        try (InputStream in = new java.io.FileInputStream(file)) {
            byte[] buffer = new byte[1 << 16];
            for (int read = in.read(buffer); read > 0; read = in.read(buffer)) {
                digest.update(buffer, 0, read);
            }
        } catch (IOException unreadable) {
            return "";
        }
        return hex(digest.digest());
    }

    private static String sha1(byte[] body) {
        try {
            return hex(MessageDigest.getInstance("SHA-1").digest(body));
        } catch (Exception impossible) {
            return "";
        }
    }

    private static String hex(byte[] digest) {
        StringBuilder text = new StringBuilder(digest.length * 2);
        for (byte one : digest) {
            text.append(Character.forDigit((one >> 4) & 0xf, 16));
            text.append(Character.forDigit(one & 0xf, 16));
        }
        return text.toString();
    }

    private Mojang() {
    }
}
