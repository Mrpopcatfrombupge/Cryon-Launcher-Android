package dev.cryon.client;

import android.app.Activity;
import android.app.AlertDialog;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.ArrayAdapter;
import android.widget.ImageView;
import android.widget.ListView;
import android.widget.TextView;
import android.widget.Toast;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * The versions, in the two kinds of thing they are.
 *
 * One section is the versions this build *is*: the Rust game, which implements a
 * Minecraft version by porting it, so the list is short and every row is a promise the
 * game keeps. The other is Mojang's own list — every version they have published, read
 * from their servers, which is what a Java run would choose from. Keeping them apart is
 * the honest version of a version picker: one list of what works, one of what exists.
 *
 * Files come from Mojang's endpoints either way, checked against the hash their list
 * gives, and a download can be stopped and finished later.
 */
public class VersionsActivity extends Activity {
    /**
     * The versions this build of the game can run.
     *
     * This mirrors `VANILLA_VERSION` in `crates/cryon-core/src/version.rs` and the data
     * version the game refuses anything else for: a row belongs here when the port
     * exists, not when Mojang published something.
     */
    private static final String[] RUST = {"26.3"};

    private static final String[] SECTIONS = {"Cryon (Rust)", "Minecraft Java"};
    /** Where Mojang's list is kept, so the screen has something to show offline. */
    private static final String CACHE = "mojang-versions.json";

    /** One row, and what tapping it does. */
    private static final class Entry {
        final String title;
        final String detail;
        final String state;
        final boolean good;
        final Runnable action;

        Entry(String title, String detail, String state, boolean good, Runnable action) {
            this.title = title;
            this.detail = detail;
            this.state = state;
            this.good = good;
            this.action = action;
        }
    }

    private final List<Entry> entries = new ArrayList<>();
    private Rows rows;
    private Cryon cryon;
    private int section = 0;
    private List<Mojang.Release> mojang = new ArrayList<>();
    private String mojangNote = "Reading Mojang's list…";
    private final Handler main = new Handler(Looper.getMainLooper());

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        setContentView(R.layout.screen_list);
        findViewById(R.id.instance_row).setVisibility(View.GONE);
        findViewById(R.id.kind_row).setVisibility(View.VISIBLE);
        ((TextView) findViewById(R.id.title)).setText(R.string.menu_versions);
        findViewById(R.id.back).setOnClickListener(v -> leave());
        cryon = Cryon.open(this);

        ListView list = findViewById(R.id.list);
        rows = new Rows();
        list.setAdapter(rows);
        list.setOnItemClickListener((parent, view, position, id) -> {
            Entry entry = entries.get(position);
            if (entry.action != null) {
                entry.action.run();
            }
        });
        buildSections();
        load();
        fetch();
    }

    private void buildSections() {
        Screens.strip(this, findViewById(R.id.kind_strip), Arrays.asList(SECTIONS), section,
                index -> {
                    section = index;
                    buildSections();
                    load();
                });
    }

    /** The rows for the section showing, from the disk and from whatever list we have. */
    private void load() {
        entries.clear();
        entries.addAll(section == 0 ? rust() : java());
        rows.notifyDataSetChanged();
        ((TextView) findViewById(R.id.empty)).setText(section == 0
                ? "No version of the Rust game is on this device."
                : mojangNote);
        findViewById(R.id.empty).setVisibility(entries.isEmpty() ? View.VISIBLE : View.GONE);
        ((TextView) findViewById(R.id.count)).setText(section == 0
                ? "This build runs " + String.join(", ", RUST)
                : mojangNote);
    }

    /** The versions this build can play, and whether their files are here.
     *
     *  Only what this build can actually run is listed: a folder copied in for a
     *  version the port does not cover is not a version the player can download
     *  or play, and a row saying NOT READY for it was an invitation to a dead
     *  end. */
    private List<Entry> rust() {
        List<Entry> found = new ArrayList<>();
        String chosen = cryon.versionLabel();
        for (String label : RUST) {
            File dir = cryon.versionDir(label);
            String missing = Cryon.missing(dir);
            if (missing.isEmpty()) {
                found.add(new Entry(label, Worlds.size(dir) + "  ·  prepared files",
                        label.equals(chosen) ? "IN USE" : "READY", true,
                        () -> choose(label)));
                continue;
            }
            if (dir.isDirectory()) {
                // Copied in, but not whole. The game would stop at this file, so
                // the row names it instead of claiming the version needs preparing.
                found.add(new Entry(label, "Copied in, but no " + missing,
                        "NOT READY", false, () -> explain(label, missing)));
                continue;
            }
            found.add(new Entry(label, "Mojang's files for this version are not here",
                    "GET", false, () -> offer(label, false)));
        }
        return found;
    }

    /** Mojang's own list, with what is on the device marked. */
    private List<Entry> java() {
        List<Entry> found = new ArrayList<>();
        for (Mojang.Release release : mojang) {
            File jar = new File(cryon.versionDir(release.id), release.id + ".jar");
            found.add(new Entry(release.id,
                    release.type + "  ·  " + release.day(),
                    jar.isFile() ? "ON THIS DEVICE" : "MOJANG", jar.isFile(),
                    () -> offer(release.id, true)));
        }
        return found;
    }

    /** The version PLAY will run. */
    private void choose(String label) {
        try {
            cryon.setVersion(label);
            leave();
        } catch (Exception refused) {
            toast("Could not remember the choice: " + refused.getMessage());
        }
    }

    /**
     * Ask before fetching, in the words a person needs to decide: how much, from where,
     * and what it will and will not do.
     *
     * A version of the Java game cannot be run by this build yet — the Java runtime for
     * Android is not in it — and saying that after 580 MB of downloading would not be a
     * good way to make a friend.
     */
    private void offer(String label, boolean java) {
        String what = java
                ? "Mojang's own files for Minecraft " + label + ": the game jar, its "
                        + "libraries and its assets, about 600 MB.\n\nThis build cannot "
                        + "run the Java game yet, because it has no Java runtime for "
                        + "Android. Downloading now means the files are ready when it has."
                : "Mojang's own files for Minecraft " + label + ": the game jar and its "
                        + "assets, about 600 MB, from Mojang's servers.\n\nThe Rust game "
                        + "also needs a converted index for the version, which is built "
                        + "by running Minecraft's Java code — that happens on a computer, "
                        + "not on a phone.";
        if ("refuse".equals(meteredChoice()) && metered()) {
            new AlertDialog.Builder(this)
                    .setTitle(label)
                    .setMessage("Launcher settings says never download over a metered "
                            + "network. Change it there if you mean to.")
                    .setNegativeButton(R.string.close, null)
                    .show();
            return;
        }
        if (metered()) {
            what += "\n\nYou are on a metered network, and this is about 600 MB of it.";
        }
        new AlertDialog.Builder(this)
                .setTitle("Download " + label + "?")
                .setMessage(what)
                .setPositiveButton("Download", (dialog, which) -> start(label, java))
                .setNegativeButton(R.string.close, null)
                .show();
    }

    /** What Launcher settings says about a metered network: ask, allow or refuse. */
    private String meteredChoice() {
        return cryon.preference("CRYON_METERED", "ask");
    }

    /**
     * Whether the network in use counts the bytes.
     *
     * A phone on mobile data is about to spend a gigabyte on a game it may not be able
     * to run yet, and a carrier that quietly bills for that is not a bug the player
     * should discover afterwards.
     */
    private boolean metered() {
        try {
            android.net.ConnectivityManager manager = (android.net.ConnectivityManager)
                    getSystemService(CONNECTIVITY_SERVICE);
            if (manager == null) {
                return false;
            }
            android.net.Network network = manager.getActiveNetwork();
            if (network == null) {
                return false;
            }
            android.net.NetworkCapabilities caps = manager.getNetworkCapabilities(network);
            return caps != null && !caps.hasCapability(
                    android.net.NetworkCapabilities.NET_CAPABILITY_NOT_METERED);
        } catch (SecurityException notGiven) {
            // Without the permission an app cannot ask; assume the friendlier answer and
            // let the size in the dialog do the warning instead.
            return false;
        }
    }

    private void explain(String label, String missing) {
        new AlertDialog.Builder(this)
                .setTitle(label)
                .setMessage("The game loads this folder and never finds " + missing
                        + ". A prepared folder is made on a computer by cryon-prepare "
                        + "and has to be copied in whole: part of it leaves the version "
                        + "unplayable, and only a computer can finish it.")
                .setNegativeButton(R.string.close, null)
                .show();
    }

    /** Begin the fetch, then let the rows follow it. */
    private void start(String label, boolean java) {
        Mojang.Release release = find(label);
        if (release == null) {
            toast("Mojang's list is not on this device yet, so " + label
                    + " cannot be found in it. Open this screen again once there is "
                    + "a connection.");
            return;
        }
        try {
            Downloads.start(cryon.files(), release, java);
        } catch (Exception failed) {
            toast("Could not start: " + failed.getMessage());
            return;
        }
        poll();
    }

    /**
     * The list's entry for a version, or null.
     *
     * Every download needs the URL Mojang gives for that version's own file, and the
     * only honest source for it is their list — so a version that is not in the list
     * we have is not downloadable, and saying that is better than guessing an address
     * and writing a half-file.
     */
    private Mojang.Release find(String label) {
        for (Mojang.Release release : mojang) {
            if (release.id.equals(label)) {
                return release;
            }
        }
        return null;
    }

    /** Refresh the rows while a download runs, and say when it stops. */
    private void poll() {
        main.postDelayed(() -> {
            Downloads.Job job = Downloads.current();
            if (job == null) {
                load();
                return;
            }
            load();
            // The line under the list carries the job's own words. A toast is gone in two
            // seconds, and a download that failed at the fortieth minute should be
            // readable rather than remembered.
            ((TextView) findViewById(R.id.count)).setText(job.running
                    ? job.line() : job.version + ": " + job.error);
            if (job.running) {
                poll();
            } else {
                Downloads.clear();
                toast(job.error == null
                        ? job.version + " is on this device" : job.error);
            }
        }, 500);
    }

    /**
     * Mojang's list, from the network when there is one and from the disk when there
     * is not.
     *
     * The saved copy is shown immediately and labelled as such: a version screen that
     * is empty on a train is not much use, but one that shows an old list without
     * saying so is worse.
     */
    private void fetch() {
        File cache = new File(cryon.files(), CACHE);
        if (cache.isFile()) {
            try {
                mojang = Mojang.parse(java.nio.file.Files.readAllBytes(cache.toPath()));
                mojangNote = "Mojang's list, as saved on "
                        + new java.util.Date(cache.lastModified()).toString();
                load();
            } catch (Exception unreadable) {
                mojangNote = "The saved list could not be read.";
            }
        }
        Thread reader = new Thread(() -> {
            try {
                byte[] body = Mojang.get(Mojang.MANIFEST);
                List<Mojang.Release> fresh = Mojang.parse(body);
                try (java.io.FileOutputStream out = new java.io.FileOutputStream(cache)) {
                    out.write(body);
                }
                main.post(() -> {
                    mojang = fresh;
                    mojangNote = "Mojang's list, read just now: " + fresh.size()
                            + " versions";
                    if (section == 1) {
                        load();
                    }
                });
            } catch (Exception offline) {
                main.post(() -> {
                    if (mojang.isEmpty()) {
                        mojangNote = "No connection, and no list saved on this device.";
                    }
                    if (section == 1) {
                        load();
                    }
                });
            }
        }, "cryon-versions");
        reader.setDaemon(true);
        reader.start();
    }

    private void leave() {
        finish();
        overridePendingTransition(R.anim.enter_rise, R.anim.exit_sink);
    }

    private void toast(String text) {
        Toast.makeText(this, text, Toast.LENGTH_LONG).show();
    }

    private class Rows extends ArrayAdapter<Entry> {
        Rows() {
            super(VersionsActivity.this, R.layout.text_row, R.id.row_title, entries);
        }

        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            View row = convertView != null ? convertView
                    : LayoutInflater.from(parent.getContext())
                            .inflate(R.layout.text_row, parent, false);
            Entry entry = getItem(position);
            ((ImageView) row.findViewById(R.id.row_mark))
                    .setImageResource(section == 0 ? R.drawable.ic_worlds
                            : R.drawable.ic_servers);
            ((TextView) row.findViewById(R.id.row_title)).setText(entry.title);
            ((TextView) row.findViewById(R.id.row_detail)).setText(entry.detail);
            TextView state = row.findViewById(R.id.row_state);
            state.setText(entry.state);
            state.setTextColor(getColor(entry.good ? R.color.shadow_accent
                    : R.color.shadow_faint));
            return row;
        }
    }
}
