package dev.cryon.client;

import java.io.File;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * The download that is running, so any screen can talk about it.
 *
 * One at a time: a phone pulling 580 MB of assets while another download competes for
 * the same link heats up and finishes neither. The job is kept in one static place
 * because the screen that started it may be gone by the time it finishes — the player
 * leaves the Versions screen to look at something else, and PLAY still has to know
 * that a download is at 42% and that it can be stopped.
 *
 * There are two stages because Mojang's list of files is itself a file: the version
 * names its jar and its asset index, and only the index says what the fifteen thousand
 * objects after it are. The second stage is the one that takes the afternoon.
 */
public final class Downloads {
    /** What is being fetched, and how far it has got. */
    public static final class Job {
        public final String version;
        /** `rust` for a version of the game this build runs, `java` for Mojang's own. */
        public final String kind;
        public volatile String stage = "Game files";
        /** The counters the workers tick live, so the button moves while a stage is
         *  still running instead of only when the stage is over. */
        public final java.util.concurrent.atomic.AtomicLong filesDone =
                new java.util.concurrent.atomic.AtomicLong();
        public final java.util.concurrent.atomic.AtomicLong bytesDone =
                new java.util.concurrent.atomic.AtomicLong();
        public volatile long filesTotal;
        /** What the whole job weighs, when the files say their size: the denominator
         *  that lets the percent move while one big jar is still arriving. */
        public volatile long bytesTotal;
        public volatile boolean running = true;
        public volatile String error;
        /** Files that could not be had; a missing sound is not a broken game. */
        public volatile int missing;

        Job(String version, String kind) {
            this.version = version;
            this.kind = kind;
        }

        /** The percent a button shows.
         *
         *  By bytes when the plan knows what the files weigh: the first file of a
         *  version is the client jar, hundreds of megabytes in one piece, and a
         *  count of files cannot move until it is done — which read as a download
         *  stuck at 0% when it was only on its first file. Where the sizes are not
         *  known it falls back to the count of files. */
        public int percent() {
            if (bytesTotal > 0) {
                return (int) Math.min(100L, bytesDone.get() * 100 / bytesTotal);
            }
            return filesTotal <= 0 ? 0 : (int) (filesDone.get() * 100 / filesTotal);
        }

        /** `Sounds and textures  ·  1 234 of 3 120 files`, the words under the button. */
        public String line() {
            return filesTotal <= 0 ? stage
                    : stage + "  ·  " + filesDone.get() + " of " + filesTotal + " files";
        }
    }

    private static Job current;
    private static final AtomicBoolean cancel = new AtomicBoolean(false);
    private static final List<Runnable> watchers = new ArrayList<>();

    /**
     * Start fetching a version.
     *
     * Every network call, including making the plan, happens on the worker: Android
     * forbids a screen thread from fetching at all, and a version file that could not be
     * read is honest as a job that failed, which is what the screen already shows.
     */
    public static Job start(File dir, Mojang.Release release, boolean java) {
        Job job = new Job(release.id, java ? "java" : "rust");
        job.stage = "Asking Mojang which files it has";
        current = job;
        cancel.set(false);
        Thread worker = new Thread(() -> {
            try {
                run(job, dir, Mojang.plan(release, dir, java));
            } catch (Exception failed) {
                job.error = failed.getMessage();
                finish(job);
            }
        }, "cryon-download");
        worker.setDaemon(true);
        worker.start();
        return job;
    }

    private static void run(Job job, File dir, List<Mojang.Item> files) {
        job.filesTotal = files.size();
        job.bytesTotal = weigh(files);
        job.stage = "Game files";
        Mojang.Progress progress = new Mojang.Progress(job.filesDone, job.bytesDone);
        List<Mojang.Item> failed = new ArrayList<>(stage(job, files, progress));
        File index = index(files);
        if (index != null && index.isFile() && !cancel.get()) {
            job.stage = "Sounds and textures";
            try {
                List<Mojang.Item> objects = Mojang.objects(new File(dir, "assets"), index);
                job.filesTotal += objects.size();
                job.bytesTotal += weigh(objects);
                failed.addAll(stage(job, objects, progress));
                layAssetsOut(job, dir, index);
            } catch (IOException couldNotList) {
                job.error = couldNotList.getMessage();
            }
        }
        job.missing = failed.size();
        if (cancel.get()) {
            job.error = "Stopped. Starting it again takes it from where it stopped.";
        } else if (!failed.isEmpty() && job.error == null) {
            job.error = failed.size() + " of " + job.filesTotal + " files could not be"
                    + " fetched. Starting again gets the rest.";
        }
        finish(job);
    }

    /** What a stage of the plan weighs, from the sizes the files declare. */
    private static long weigh(List<Mojang.Item> files) {
        long total = 0;
        for (Mojang.Item item : files) {
            if (item.size > 0) {
                total += item.size;
            }
        }
        return total;
    }

    /** Say the job is over, to whoever is watching it. */
    private static void finish(Job job) {
        job.running = false;
        synchronized (watchers) {
            for (Runnable watcher : watchers) {
                watcher.run();
            }
        }
    }

    /**
     * Fetch one stage. The counters are the job's own, so every file and byte the
     * workers land is visible to the button the moment it lands.
     */
    private static List<Mojang.Item> stage(Job job, List<Mojang.Item> files,
            Mojang.Progress progress) {
        return Mojang.download(files, progress, cancel);
    }

    /**
     * Copy the objects into the tree the game reads, once the game's own folder is here.
     *
     * The version folder holds the converted index, which is the part a computer makes.
     * When that has arrived — copied in, or sent from a PC — the assets downloaded here
     * belong beside it, and the version is whole without a second download.
     */
    private static void layAssetsOut(Job job, File dir, File index) {
        File prepared = new File(new File(new File(dir, "versions"), job.version),
                "cryon-resources.json");
        if (!prepared.isFile() || cancel.get()) {
            return;
        }
        job.stage = "Putting the files where the game looks";
        Mojang.Progress progress = new Mojang.Progress();
        try {
            Mojang.virtualise(index, new File(prepared.getParentFile(), "assets"),
                    progress, cancel);
        } catch (IOException couldNotCopy) {
            job.error = couldNotCopy.getMessage();
        }
    }

    /** The asset index among the planned files: the one that lists all the others. */
    private static File index(List<Mojang.Item> files) {
        for (Mojang.Item item : files) {
            if (item.target.getAbsolutePath().replace('\\', '/')
                    .contains("assets/indexes/")) {
                return item.target;
            }
        }
        return null;
    }

    /** Stop the download under way; the files already finished stay finished. */
    public static void cancel() {
        cancel.set(true);
    }

    /** The job PLAY should be describing, or null when nothing is running or just ran. */
    public static Job current() {
        return current;
    }

    /** Forget the finished job, so PLAY goes back to saying PLAY. */
    public static void clear() {
        if (current != null && !current.running) {
            current = null;
        }
    }

    /** Called on the download thread when it ends; register on the main thread only. */
    public static void watch(Runnable watcher) {
        synchronized (watchers) {
            watchers.add(watcher);
        }
    }

    public static void unwatch(Runnable watcher) {
        synchronized (watchers) {
            watchers.remove(watcher);
        }
    }

    private Downloads() {
    }
}
