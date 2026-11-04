package dev.cryon.client;

import android.app.Activity;
import android.app.AlertDialog;
import android.os.Bundle;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.ArrayAdapter;
import android.widget.ImageView;
import android.widget.ListView;
import android.widget.TextView;
import java.io.File;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * The app's own folder, as it is on the disk.
 *
 * Everything this launcher and the game share lives in one place —
 * `Android/data/dev.cryon.client/files` — and a phone with a cable in it can read
 * it, which is how a version, a world or a mod gets onto the device in the first
 * place. This screen is the same view of that folder, so "copy it in" is something
 * a player can check rather than take on trust.
 *
 * It shows, names and deletes. It does not write: a file manager that can edit a
 * `level.dat` is a file manager that can ruin one, and the game writes its own files
 * better than anything here could.
 */
public class FilesActivity extends Activity {
    private File root;
    private File dir;
    private final List<File> entries = new ArrayList<>();
    private Rows rows;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        setContentView(R.layout.screen_list);
        findViewById(R.id.instance_row).setVisibility(View.GONE);
        ((TextView) findViewById(R.id.title)).setText(R.string.menu_files);
        ((TextView) findViewById(R.id.empty)).setText("Nothing in this folder.");
        findViewById(R.id.back).setOnClickListener(v -> up());
        root = Cryon.open(this).files();
        dir = root;

        ListView list = findViewById(R.id.list);
        rows = new Rows();
        list.setAdapter(rows);
        list.setOnItemClickListener((parent, view, position, id) -> open(position));
        list.setOnItemLongClickListener((parent, view, position, id) -> {
            confirmDelete(entries.get(position));
            return true;
        });
        show();
    }

    /** Folders first, then files, each by name — the order a person looks in. */
    private void show() {
        File[] listed = dir.listFiles();
        entries.clear();
        if (listed != null) {
            List<File> folders = new ArrayList<>();
            List<File> files = new ArrayList<>();
            for (File file : listed) {
                if (file.isDirectory()) {
                    folders.add(file);
                } else {
                    files.add(file);
                }
            }
            byName(folders);
            byName(files);
            entries.addAll(folders);
            entries.addAll(files);
        }
        rows.notifyDataSetChanged();
        findViewById(R.id.empty).setVisibility(entries.isEmpty() ? View.VISIBLE : View.GONE);
        ((TextView) findViewById(R.id.count)).setText(where());
    }

    private static void byName(List<File> files) {
        files.sort((a, b) -> a.getName().compareToIgnoreCase(b.getName()));
    }

    /** The path from the app's folder down to here, which is what a cable needs. */
    private String where() {
        String tail = dir.getAbsolutePath().substring(root.getAbsolutePath().length());
        return "Android/data/dev.cryon.client/files" + tail;
    }

    private void open(int position) {
        File file = entries.get(position);
        if (file.isDirectory()) {
            dir = file;
            show();
            return;
        }
        describe(file);
    }

    private void describe(File file) {
        StringBuilder text = new StringBuilder();
        text.append("Size:  ").append(Worlds.size(file)).append('\n');
        text.append("Modified:  ").append(when(file.lastModified())).append('\n');
        text.append('\n').append(file.getAbsolutePath());
        new AlertDialog.Builder(this)
                .setTitle(file.getName())
                .setMessage(text.toString())
                .setPositiveButton("Delete", (dialog, which) -> confirmDelete(file))
                .setNegativeButton(R.string.close, null)
                .show();
    }

    private String when(long millis) {
        return java.text.DateFormat.getDateTimeInstance(
                java.text.DateFormat.MEDIUM, java.text.DateFormat.SHORT)
                .format(new java.util.Date(millis));
    }

    private void confirmDelete(File file) {
        boolean folder = file.isDirectory();
        new AlertDialog.Builder(this)
                .setTitle("Delete " + file.getName() + "?")
                .setMessage(folder
                        ? "This folder and everything inside it (" + Worlds.size(file)
                                + ") goes, including any world, instance or version"
                                + " it holds."
                        : "This file is " + Worlds.size(file) + ".")
                .setPositiveButton("Delete", (dialog, which) -> {
                    Cryon.deleteAll(file);
                    show();
                })
                .setNegativeButton(R.string.close, null)
                .show();
    }

    /** Back walks out of one folder, and only leaves the screen once it is at the top. */
    private void up() {
        if (dir.equals(root)) {
            leave();
            return;
        }
        File parent = dir.getParentFile();
        dir = parent == null || !parent.canRead() ? root : parent;
        show();
    }

    @Override
    public void onBackPressed() {
        up();
    }

    private void leave() {
        finish();
        overridePendingTransition(R.anim.enter_rise, R.anim.exit_sink);
    }

    private class Rows extends ArrayAdapter<File> {
        Rows() {
            super(FilesActivity.this, R.layout.text_row, R.id.row_title, entries);
        }

        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            View row = convertView != null ? convertView
                    : LayoutInflater.from(parent.getContext())
                            .inflate(R.layout.text_row, parent, false);
            File file = getItem(position);
            boolean folder = file.isDirectory();
            ((ImageView) row.findViewById(R.id.row_mark)).setImageResource(
                    folder ? R.drawable.ic_folder : R.drawable.ic_file_row);
            ((TextView) row.findViewById(R.id.row_title)).setText(file.getName());
            ((TextView) row.findViewById(R.id.row_detail)).setText(
                    (folder ? Worlds.size(file) : Worlds.humanSize(file.length()))
                            + "  ·  " + when(file.lastModified()));
            ((TextView) row.findViewById(R.id.row_state)).setText("");
            return row;
        }
    }
}
