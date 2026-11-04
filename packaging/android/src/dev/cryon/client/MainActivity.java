package dev.cryon.client;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.graphics.drawable.AnimatedVectorDrawable;
import android.os.Bundle;
import android.view.LayoutInflater;
import android.view.View;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.Toast;
import java.io.File;
import java.util.ArrayList;
import java.util.List;

/**
 * The launcher's front screen.
 *
 * Three zones, in Pojav's order: the account and the way into settings at the top,
 * the things you can do in the middle, and the two choices that lead to PLAY at the
 * bottom, on a plate of their own and under the thumb. Nothing is hidden behind a
 * hamburger here — a launcher whose sections only appear once you open a drawer
 * makes you work to find out what it can do.
 *
 * The screen itself starts nothing. It writes the choice into `files/env.txt` and
 * into `instance.txt`, then hands over to the activity that loads the engine.
 */
public class MainActivity extends Activity {
    /** The middle of the screen: label and icon, in the order they are listed. */
    private static final int[][] MENU = {
        {R.string.menu_instances, R.drawable.ic_instances},
        {R.string.menu_settings, R.drawable.ic_settings},
        {R.string.menu_launcher, R.drawable.ic_launcher_settings},
        {R.string.menu_controls, R.drawable.ic_controls},
        {R.string.menu_versions, R.drawable.ic_versions},
        {R.string.menu_worlds, R.drawable.ic_worlds},
        {R.string.menu_servers, R.drawable.ic_servers},
        {R.string.menu_mods, R.drawable.ic_mods},
        {R.string.menu_files, R.drawable.ic_files},
        {R.string.menu_credits, R.drawable.ic_credits},
    };

    /**
     * The screen each row leads to, in the same order as the rows.
     *
     * Every one of them is built now, which is the point: a launcher whose menu
     * answers "not built yet" is a list of promises rather than a launcher.
     */
    private static final Class<?>[] SCREENS = {
        InstancesActivity.class,
        SettingsActivity.class,
        SettingsActivity.class,
        ControlsActivity.class,
        VersionsActivity.class,
        WorldsActivity.class,
        ServersActivity.class,
        ModsActivity.class,
        FilesActivity.class,
        CreditsActivity.class,
    };

    private Cryon cryon;
    private List<Cryon.Version> versions = new ArrayList<>();
    /** The version PLAY will run, by name — see {@link #current()}. */
    private String chosen = "";
    private TextView instanceChip;
    private TextView versionChip;
    private View versionEdit;
    private TextView play;
    private final android.os.Handler main = new android.os.Handler(
            android.os.Looper.getMainLooper());

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        setContentView(R.layout.main);
        cryon = Cryon.open(this);

        instanceChip = findViewById(R.id.instance_chip);
        versionChip = findViewById(R.id.version_chip);
        versionEdit = findViewById(R.id.version_edit);
        play = findViewById(R.id.play);

        findViewById(R.id.account_chip).setOnClickListener(v -> name());
        findViewById(R.id.settings_button).setOnClickListener(v -> open(1));
        findViewById(R.id.instance_row).setOnClickListener(v -> open(0));
        versionChip.setOnClickListener(v -> versions());
        versionEdit.setOnClickListener(v -> versions());
        play.setOnClickListener(v -> press());

        ((TextView) findViewById(R.id.account_name)).setText(accountLabel());
        buildMenu();
    }

    /** The rows in the middle, each with the icon drawn for it. */
    private void buildMenu() {
        LinearLayout menu = findViewById(R.id.menu);
        LayoutInflater inflater = LayoutInflater.from(this);
        menu.removeAllViews();
        int[][] sections = {{5, 6}, {0, 4, 7}, {1, 3}, {8, 9}};
        String[] titles = {"PLAY YOUR WAY", "YOUR LIBRARY", "PREFERENCES", "TOOLS"};
        boolean compact = "1".equals(cryon.preference("launcherCompact", "0"));
        float scale;
        try { scale = Math.max(.85f, Math.min(1.25f, Float.parseFloat(cryon.preference("launcherTextScale", "100")) / 100)); }
        catch (NumberFormatException bad) { scale = 1f; }
        int accent = accent();
        for (int section = 0; section < sections.length; section++) {
            TextView header = new TextView(this);
            header.setText(titles[section]);
            header.setTextSize(11 * scale);
            header.setLetterSpacing(.1f);
            header.setTextColor(accent);
            header.setPadding(dp(20), dp(14), dp(16), dp(6));
            menu.addView(header);
            for (int position : sections[section]) {
                View row = inflater.inflate(R.layout.menu_row, menu, false);
                ImageView icon = row.findViewById(R.id.row_icon);
                icon.setImageResource(MENU[position][1]);
                icon.setColorFilter(accent);
                TextView label = row.findViewById(R.id.row_label);
                label.setText(position == 1 ? "Settings" : position == 7 ? "Mods & packs" : getString(MENU[position][0]));
                label.setTextSize(15 * scale);
                row.getLayoutParams().height = dp(compact ? 48 : 56);
                row.setFocusable(true);
                row.setOnClickListener(v -> open(position));
                menu.addView(row);
            }
            if (section == sections.length - 1) {
                View row = inflater.inflate(R.layout.menu_row, menu, false);
                ImageView icon = row.findViewById(R.id.row_icon);
                icon.setImageResource(R.drawable.ic_launcher_settings);
                icon.setColorFilter(accent);
                TextView label = row.findViewById(R.id.row_label);
                label.setText("Updates");
                label.setTextSize(15 * scale);
                row.getLayoutParams().height = dp(compact ? 48 : 56);
                row.setFocusable(true);
                row.setOnClickListener(v -> Updates.check(this));
                menu.addView(row);
            }
        }
    }

    /**
     * What is on the device, and whether PLAY can be pressed.
     *
     * Runs again on every resume, because the honest answer to "where are my
     * versions" is a folder a person may have filled in from a file manager while
     * the app was closed — and because the instance you come back to may have been
     * changed on the Instances screen.
     */
    @Override
    protected void onResume() {
        super.onResume();
        scan();
        buildMenu();
        logo(true);
        ShadowBackdrop shadow = findViewById(R.id.backdrop);
        if (shadow != null) {
            shadow.refreshMotion();
        }
        watch(true);
    }

    @Override
    protected void onPause() {
        watch(false);
        logo(false);
        super.onPause();
    }

    private void scan() {
        versions = cryon.versions();
        instanceChip.setText(cryon.instance());
        ((TextView) findViewById(R.id.account_name)).setText(accountLabel());
        // The file is the truth about the choice: the Versions screen writes it, and
        // this screen is resumed after that, not restarted.
        String remembered = cryon.versionLabel();
        if (!remembered.isEmpty()) {
            chosen = remembered;
        }
        if (current() == null) {
            Cryon.Version first = cryon.chosen();
            chosen = first == null ? "" : first.label;
        }
        show();
    }

    /**
     * The version chosen, found by its name.
     *
     * Not by position: the list is the folders on the device, and a download that
     * finished while this screen was closed puts a new one at the top. A PLAY that
     * remembered an index would then start a different version than the one the chip
     * says it is running.
     */
    private Cryon.Version current() {
        for (Cryon.Version version : versions) {
            if (version.label.equals(chosen)) {
                return version;
            }
        }
        return null;
    }

    private void show() {
        Cryon.Version version = current();
        if (version == null) {
            versionChip.setText(versions.isEmpty()
                    ? getString(R.string.empty_versions)
                    : chosen + "  (not on this device)");
            play.setEnabled(false);
            return;
        }
        versionChip.setText(version.playable
                ? version.label
                : version.label + "  (no " + part(version.missing) + ")");
        play.setEnabled(version.playable);
    }

    /** The last piece of a path, which is the part a player can act on. */
    private static String part(String path) {
        int slash = path.lastIndexOf('/');
        return slash < 0 ? path : path.substring(slash + 1);
    }

    /** The Versions screen: what this build runs, and everything Mojang has published. */
    private void versions() {
        enter(new Intent(this, VersionsActivity.class));
    }

    /**
     * The one button. While a download is running it is not a play button: it says how
     * far the download is, and pressing it stops the download. A second button for
     * Cancel would have been a second thing to find while the first one was already
     * telling you something was happening.
     */
    private void press() {
        Downloads.Job job = Downloads.current();
        if (job != null && job.running) {
            Downloads.cancel();
            toast("Stopped. Starting it again takes it from where it stopped.");
            return;
        }
        startGame();
    }

    private final Runnable tick = new Runnable() {
        @Override
        public void run() {
            Downloads.Job job = Downloads.current();
            if (job == null) {
                play.setText(R.string.play);
                return;
            }
            if (job.running) {
                play.setText("GETTING READY  " + job.percent() + "%");
                play.setEnabled(true);
                main.postDelayed(this, 400);
                return;
            }
            Downloads.clear();
            play.setText(R.string.play);
            scan();
            toast(job.error == null ? job.version + " is on this device" : job.error);
        }
    };

    /**
     * Watch the download while this screen is up.
     *
     * Stopped on the way out rather than left turning: a post that keeps a dead screen
     * alive is how a launcher ends up holding an Activity and its bitmaps in memory
     * while the player is in the game.
     */
    private void watch(boolean going) {
        main.removeCallbacks(tick);
        if (going) {
            main.post(tick);
        }
    }

    /**
     * The handoff: say what to load, then start the activity that loads it.
     *
     * Writing the file is the whole launch. If it cannot be written the button has
     * done nothing, and saying so is better than the black screen that is what a
     * failed start used to look like.
     */
    private void startGame() {
        Cryon.Version version = current();
        if (version == null) {
            toast("No version on this device yet.");
            return;
        }
        if (!version.playable) {
            toast(version.label + " has no " + part(version.missing));
            return;
        }
        try {
            cryon.writeStart(version.dir.getAbsolutePath(), controls());
        } catch (Exception failure) {
            toast("could not write the start: " + failure.getMessage());
            return;
        }
        // The mods' Java behaviour runtime is started by Cryon.launch, which
        // every start walks through — PLAY, a world, a server.
        cryon.launch(this);
    }

    /**
     * Where the game finds its controls, and how big a dp is on this phone.
     *
     * The layout the editor writes is the file the game reads, so a new player's
     * default layout is put on disk first: the game has no copy of the defaults of
     * its own, and a missing file would mean a screen with nothing to press.
     */
    private List<String> controls() {
        String name = cryon.preference("defaultCtrl", "default");
        Layout layout = Layout.load(cryon.files(), name);
        File file = new File(Layout.folder(cryon.files()), name + ".json");
        try {
            layout.save(cryon.files(), name);
        } catch (Exception failure) {
            toast("the layout could not be written: " + failure);
        }
        return java.util.Arrays.asList(
                "CRYON_CONTROLS=" + file.getAbsolutePath(),
                "CRYON_DENSITY=" + getResources().getDisplayMetrics().density);
    }

    /** The screen a row leads to. */
    private void open(int position) {
        Intent screen = new Intent(this, SCREENS[position]);
        if (MENU[position][0] == R.string.menu_launcher) {
            // The same screen, the other half of the settings: the rows are one list
            // and the tabs decide which of them this screen shows.
            screen.putExtra(SettingsActivity.LAUNCHER, true);
        }
        enter(screen);
    }

    /**
     * Moving to a category: the new screen rises out of the dark as the old one
     * sinks into it.
     */
    private void enter(Intent screen) {
        startActivity(screen);
        overridePendingTransition(R.anim.enter_rise, R.anim.exit_sink);
    }

    /**
     * The mark breathes while the screen is up and stops when it is not.
     *
     * An animated drawable keeps running once started, including behind another
     * app, so it is restarted on the way back in rather than left turning in the
     * dark, costing frames nobody can see.
     */
    private void logo(boolean going) {
        ImageView mark = findViewById(R.id.logo);
        if (mark.getDrawable() instanceof AnimatedVectorDrawable) {
            AnimatedVectorDrawable live = (AnimatedVectorDrawable) mark.getDrawable();
            if (going && "1".equals(cryon.preference("launcherMotion", "0"))
                    && android.animation.ValueAnimator.areAnimatorsEnabled()) {
                live.start();
            } else {
                live.stop();
            }
        }
    }

    /**
     * The name the player goes by, which belongs to the instance they are in.
     *
     * There is no Microsoft sign-in here yet, so this is the offline name the game
     * keeps for itself — but it is the player's own name, shown and changeable,
     * rather than a "not signed in" that says nothing about who they are in a world.
     */
    private String accountLabel() {
        String name = cryon.playerName();
        return name.isEmpty() ? "Name not chosen yet" : name;
    }

    /** The name, asked for in words and written where the game will read it. */
    private void name() {
        android.widget.EditText field = new android.widget.EditText(this);
        field.setText(cryon.playerName());
        field.setSingleLine(true);
        field.setHint("Player");
        new AlertDialog.Builder(this)
                .setTitle("Your name in the game")
                .setMessage("3 to 16 letters, digits or underscores. It decides which "
                        + "inventory is yours on a server that is not signed in.")
                .setView(field)
                .setPositiveButton("Keep", (dialog, which) -> {
                    try {
                        cryon.setPlayerName(field.getText().toString().trim());
                        ((TextView) findViewById(R.id.account_name)).setText(accountLabel());
                    } catch (Exception refused) {
                        toast(refused.getMessage());
                    }
                })
                .setNegativeButton(R.string.close, null)
                .show();
    }

    private int dp(int value) { return Math.round(value * getResources().getDisplayMetrics().density); }

    private int accent() {
        switch (cryon.preference("launcherAccent", "ice")) {
            case "mint": return 0xFF91D6BE;
            case "violet": return 0xFFC0ADF4;
            default: return getColor(R.color.shadow_accent);
        }
    }

    private void toast(String text) {
        Toast.makeText(this, text, Toast.LENGTH_SHORT).show();
    }
}
