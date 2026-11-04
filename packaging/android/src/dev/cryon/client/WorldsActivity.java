package dev.cryon.client;

import android.app.Activity;
import android.app.AlertDialog;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Outline;
import android.os.Bundle;
import android.util.LruCache;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewOutlineProvider;
import android.widget.ArrayAdapter;
import android.widget.ImageView;
import android.widget.ListView;
import android.widget.TextView;
import android.widget.Toast;
import java.io.File;
import java.util.ArrayList;
import java.util.List;

/**
 * The worlds, one instance at a time.
 *
 * Two things a plain folder listing cannot say: which world is which, and whose
 * worlds these are. The strip across the top answers the second — worlds belong to
 * the instance that made them, and mixing two instances' `New World`s into one list
 * is how a player ends up deleting the wrong save. The picture the game saved
 * answers the first: it is a screenshot of the actual place, so a row can be
 * recognised at a glance rather than read.
 *
 * Tapping a world starts the game in it. Holding one opens what else there is to
 * know about it, including the only destructive thing this screen can do.
 */
public class WorldsActivity extends Activity {
    private Cryon cryon;
    private File files;
    private int instance = 0;
    private List<String> instances = new ArrayList<>();
    private List<Worlds.World> worlds = new ArrayList<>();
    private Rows rows;
    private final LruCache<String, Bitmap> icons = new LruCache<String, Bitmap>(8 * 1024 * 1024) {
        @Override
        protected int sizeOf(String key, Bitmap value) {
            return value.getByteCount();
        }
    };

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        setContentView(R.layout.screen_list);
        ((TextView) findViewById(R.id.title)).setText(R.string.menu_worlds);
        ((TextView) findViewById(R.id.empty)).setText(R.string.empty_worlds);
        findViewById(R.id.back).setOnClickListener(v -> leave());
        cryon = Cryon.open(this);
        files = cryon.files();

        ListView list = findViewById(R.id.list);
        rows = new Rows();
        list.setAdapter(rows);
        list.setOnItemClickListener((parent, view, position, id) -> play(worlds.get(position)));
        list.setOnItemLongClickListener((parent, view, position, id) -> {
            ask(worlds.get(position));
            return true;
        });
    }

    /** The worlds are read again on every visit: the game may have added or renamed one. */
    @Override
    protected void onResume() {
        super.onResume();
        instances = Instances.list(files);
        int current = instances.indexOf(cryon.instance());
        instance = current < 0 ? 0 : current;
        buildStrip();
        refresh();
    }

    private void buildStrip() {
        Screens.strip(this, findViewById(R.id.instance_strip), instances, instance, index -> {
            instance = index;
            buildStrip();
            refresh();
        });
    }

    private File dir() {
        return Instances.dir(files, instances.get(instance));
    }

    private void refresh() {
        worlds.clear();
        worlds.addAll(Worlds.list(dir()));
        rows.notifyDataSetChanged();
        findViewById(R.id.empty).setVisibility(worlds.isEmpty() ? View.VISIBLE : View.GONE);
        ((TextView) findViewById(R.id.count)).setText(plural(worlds.size(), "world")
                + " in " + instances.get(instance));
    }

    /** `2 worlds`, and the one thing the two list screens agree on how to say. */
    static String plural(int count, String noun) {
        return count + " " + noun + (count == 1 ? "" : "s");
    }

    /** Start the game in this world, skipping its own title screen. */
    private void play(Worlds.World world) {
        try {
            cryon.launchWorld(this, instances.get(instance), world.dir.getName());
        } catch (Exception refused) {
            Toast.makeText(this, refused.getMessage(), Toast.LENGTH_LONG).show();
        }
    }

    /** Hold a row: everything there is to know, and the way to delete it. */
    private void ask(Worlds.World world) {
        String[] actions = {"Play", "Details", "Delete"};
        new AlertDialog.Builder(this)
                .setTitle(world.name)
                .setItems(actions, (dialog, which) -> {
                    switch (which) {
                        case 0:
                            play(world);
                            break;
                        case 1:
                            details(world);
                            break;
                        default:
                            confirmDelete(world);
                            break;
                    }
                })
                .setNegativeButton(R.string.close, null)
                .show();
    }

    /**
     * The facts the list has room for only one of. The folder name is here because
     * it is the thing a save is actually called, and the only thing worth typing
     * into a file manager.
     */
    private void details(Worlds.World world) {
        StringBuilder text = new StringBuilder();
        if (world.broken) {
            text.append("This save's level.dat could not be read.\n");
        }
        line(text, "Folder", world.dir.getName());
        line(text, "Mode", world.mode());
        line(text, "Played", Worlds.playTime(world.playTimeTicks));
        line(text, "Last", Worlds.playedOn(world.lastPlayed));
        line(text, "Size", Worlds.size(world.dir));
        line(text, "Version", world.version);
        line(text, "Seed", world.seed == null ? "" : String.valueOf(world.seed));
        new AlertDialog.Builder(this)
                .setTitle(world.name)
                .setMessage(text.toString())
                .setPositiveButton("Play", (dialog, which) -> play(world))
                .setNegativeButton(R.string.close, null)
                .show();
    }

    private static void line(StringBuilder into, String label, String value) {
        if (value != null && !value.isEmpty()) {
            into.append(label).append(":  ").append(value).append('\n');
        }
    }

    /**
     * Deleting a world is the one thing on this screen that cannot be undone, so it
     * says how much it is taking with it before it asks for permission.
     */
    private void confirmDelete(Worlds.World world) {
        new AlertDialog.Builder(this)
                .setTitle("Delete " + world.name + "?")
                .setMessage("This save is " + Worlds.size(world.dir)
                        + ". Deleting it removes the world, its buildings and its"
                        + " map — there is no way back.")
                .setPositiveButton("Delete", (dialog, which) -> {
                    Cryon.deleteAll(world.dir);
                    refresh();
                })
                .setNegativeButton(R.string.close, null)
                .show();
    }

    private void leave() {
        finish();
        overridePendingTransition(R.anim.enter_rise, R.anim.exit_sink);
    }

    /**
     * A save's picture, decoded at the size the row shows it.
     *
     * Decoded once and kept: the list recycles rows as it scrolls, and a 64-pixel
     * square read from storage again on every scroll would be the slowest part of a
     * screen whose job is to be looked at.
     */
    private Bitmap icon(Worlds.World world) {
        if (!world.hasIcon()) {
            return null;
        }
        String key = world.iconFile.getAbsolutePath() + "/" + world.iconFile.lastModified();
        Bitmap cached = icons.get(key);
        if (cached != null) {
            return cached;
        }
        BitmapFactory.Options bounds = new BitmapFactory.Options();
        bounds.inJustDecodeBounds = true;
        BitmapFactory.decodeFile(world.iconFile.getAbsolutePath(), bounds);
        BitmapFactory.Options options = new BitmapFactory.Options();
        int target = getResources().getDimensionPixelSize(R.dimen.world_thumb) * 2;
        options.inSampleSize = Math.max(1, bounds.outWidth / target);
        Bitmap loaded = BitmapFactory.decodeFile(world.iconFile.getAbsolutePath(), options);
        if (loaded != null) {
            icons.put(key, loaded);
        }
        return loaded;
    }

    /** The rows: the world's picture, its name, and what kind of save it is. */
    private class Rows extends ArrayAdapter<Worlds.World> {
        Rows() {
            super(WorldsActivity.this, R.layout.world_row, R.id.world_name, worlds);
        }

        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            View row = convertView != null ? convertView
                    : LayoutInflater.from(parent.getContext())
                            .inflate(R.layout.world_row, parent, false);
            Worlds.World world = getItem(position);
            ((TextView) row.findViewById(R.id.world_name)).setText(world.name);
            ((TextView) row.findViewById(R.id.world_detail)).setText(detail(world));
            ((TextView) row.findViewById(R.id.world_when)).setText(
                    (world.broken ? "Unreadable save"
                            : "Last played " + Worlds.playedOn(world.lastPlayed))
                            + "  ·  " + Worlds.size(world.dir));
            ImageView picture = row.findViewById(R.id.world_icon);
            Bitmap shown = icon(world);
            if (shown != null) {
                picture.setImageBitmap(shown);
            } else {
                picture.setImageResource(R.drawable.ic_worlds);
            }
            picture.setOutlineProvider(ROUND);
            picture.setClipToOutline(true);
            ((TextView) row.findViewById(R.id.world_state)).setText(
                    world.dir.getName().equals(cryon.worldToOpen())
                            && instances.get(instance).equals(cryon.instance())
                            ? "NEXT" : "");
            return row;
        }
    }

    /**
     * `Survival · 2h 14m`, or what little of it the save told us.
     *
     * The folder's own name is the fallback: a world with nothing readable in its
     * `level.dat` is still a folder the player can find in a file manager, and a
     * blank line would tell them nothing at all.
     */
    private String detail(Worlds.World world) {
        List<String> parts = new ArrayList<>();
        if (!world.mode().isEmpty()) {
            parts.add(world.mode());
        }
        if (world.playTimeTicks > 0) {
            parts.add(Worlds.playTime(world.playTimeTicks) + " played");
        }
        if (!world.version.isEmpty()) {
            parts.add(world.version);
        }
        return parts.isEmpty() ? world.dir.getName() : String.join("  ·  ", parts);
    }

    /** The picture's corners, rounded to match the frame drawn under it. */
    private static final ViewOutlineProvider ROUND = new ViewOutlineProvider() {
        @Override
        public void getOutline(View view, Outline outline) {
            outline.setRoundRect(0, 0, view.getWidth(), view.getHeight(),
                    view.getResources().getDimensionPixelSize(R.dimen.world_thumb) / 6f);
        }
    };
}
