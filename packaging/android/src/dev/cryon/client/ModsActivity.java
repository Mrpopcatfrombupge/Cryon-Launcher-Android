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
import android.widget.Toast;
import java.io.File;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * The add-ons in an instance, and the switch that says whether the game loads them.
 *
 * The engine's own rule is that a content mod is off until it is turned on, so the
 * list would be a wall of files that do nothing if the launcher only showed the
 * folder. The row is the file the player copied in, the state is the line the game
 * reads in `cryon-mods.txt`, and tapping one writes the other.
 *
 * Packs are listed but not switched: which resource pack is on is a choice the game
 * makes in its own Options screen, from a list it builds itself, and a launcher that
 * edited that would be guessing at an order the game cares about.
 */
public class ModsActivity extends Activity {
    private static final String[] KINDS = {"Mods", "Resource packs", "Shader packs"};

    private Cryon cryon;
    private File files;
    private int instance = 0;
    private int kind = 0;
    private List<String> instances = new ArrayList<>();
    private final List<Mods.Item> items = new ArrayList<>();
    private Rows rows;
    private static final int PICK_JAR = 42;
    private String importInstance;
    private static final java.util.concurrent.ExecutorService IMPORTS =
            java.util.concurrent.Executors.newSingleThreadExecutor();
    private static volatile boolean importing;
    private static volatile String importResult;
    private final android.os.Handler main = new android.os.Handler(android.os.Looper.getMainLooper());
    private android.widget.Button importButton;
    private TextView behaviorStatus;

    /** The behaviour line under the import button, refreshed live. */
    private void updateBehaviorStatus() {
        String text = "Java behaviour: " + BehaviorRuntime.status();
        behaviorStatus.setText(text);
        behaviorStatus.setTextColor(0xFF9AA3AD);
    }
    private static native String importNative(String jar, String work, String game, String resources);
    private final Runnable watchImport = new Runnable() {
        public void run() {
            if (isFinishing() || isDestroyed()) return;
            importButton.setEnabled(!importing);
            importButton.setText(importing ? "Inspecting and converting..." : "Import Fabric / Forge jar");
            if (importResult != null) {
                String result = importResult;
                importResult = null;
                showReport(result);
                refresh();
            }
            updateBehaviorStatus();
            if (importing) main.postDelayed(this, 250);
        }
    };

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        setContentView(R.layout.screen_list);
        ((TextView) findViewById(R.id.title)).setText(R.string.menu_mods);
        findViewById(R.id.back).setOnClickListener(v -> leave());
        findViewById(R.id.kind_row).setVisibility(View.VISIBLE);
        cryon = Cryon.open(this);
        files = cryon.files();
        if (saved != null) importInstance = saved.getString("importInstance");
        importButton = new android.widget.Button(this);
        importButton.setText("Import Fabric / Forge jar");
        importButton.setAllCaps(false);
        importButton.setMinHeight((int) (48 * getResources().getDisplayMetrics().density));
        android.widget.LinearLayout importContainer = (android.widget.LinearLayout) findViewById(R.id.count).getParent();
        importContainer.addView(importButton, importContainer.getChildCount() - 1);
        // What the behaviour runtime did at the last game start, so "does the
        // mod's Java run" is answered on this screen rather than in logcat.
        behaviorStatus = new TextView(this);
        behaviorStatus.setTextSize(13);
        behaviorStatus.setPadding(0, (int) (8 * getResources().getDisplayMetrics().density), 0, 0);
        importContainer.addView(behaviorStatus, importContainer.getChildCount() - 1);
        importButton.setOnClickListener(v -> new AlertDialog.Builder(this)
                .setTitle("Import compatible content")
                .setMessage("Converts supported data and assets locally. Java code, mixins, custom rendering and native libraries will NOT run. Block and item behavior may be approximated. Imports start disabled; review the report before enabling. The original jar is never changed.")
                .setPositiveButton("Choose jar", (d, which) -> {
                    importInstance = dir().getAbsolutePath();
                    android.content.Intent pick = new android.content.Intent(android.content.Intent.ACTION_OPEN_DOCUMENT);
                    pick.addCategory(android.content.Intent.CATEGORY_OPENABLE);
                    pick.setType("*/*");
                    startActivityForResult(pick, PICK_JAR);
                }).setNegativeButton("Cancel", null).show());

        ListView list = findViewById(R.id.list);
        rows = new Rows();
        list.setAdapter(rows);
        list.setOnItemClickListener((parent, view, position, id) ->
                toggle(items.get(position)));
        list.setOnItemLongClickListener((parent, view, position, id) -> {
            ask(items.get(position));
            return true;
        });
    }

    private void buildKinds() {
        Screens.strip(this, findViewById(R.id.kind_strip), Arrays.asList(KINDS), kind,
                index -> {
                    kind = index;
                    buildKinds();
                    refresh();
                });
    }

    @Override
    protected void onResume() {
        super.onResume();
        instances = Instances.list(files);
        int current = instances.indexOf(cryon.instance());
        instance = current < 0 ? 0 : current;
        buildStrip();
        buildKinds();
        refresh();
        updateBehaviorStatus();
        main.post(watchImport);
    }

    @Override protected void onPause() { main.removeCallbacks(watchImport); super.onPause(); }
    @Override protected void onSaveInstanceState(Bundle state) {
        state.putString("importInstance", importInstance);
        super.onSaveInstanceState(state);
    }

    @Override protected void onActivityResult(int request, int result, android.content.Intent data) {
        super.onActivityResult(request, result, data);
        if (request != PICK_JAR || result != RESULT_OK || data == null || data.getData() == null || importing) return;
        final android.net.Uri uri = data.getData();
        final android.content.Context context = getApplicationContext();
        final File game = new File(importInstance == null ? cryon.gameDir().getAbsolutePath() : importInstance);
        final Cryon.Version version = cryon.chosen();
        if (version == null) {
            Toast.makeText(this, "Prepare a playable version first so templates can be checked.", Toast.LENGTH_LONG).show();
            return;
        }
        final String resources = version.dir.getAbsolutePath();
        importing = true;
        IMPORTS.execute(() -> {
            File source = null;
            try {
                File job = new File(game, ".port-work/" + java.util.UUID.randomUUID());
                if (!job.mkdirs()) throw new java.io.IOException("Cannot create import staging folder");
                source = new File(job, "source.jar");
                try (java.io.InputStream in = context.getContentResolver().openInputStream(uri);
                        java.io.OutputStream out = new java.io.FileOutputStream(source)) {
                    if (in == null) throw new java.io.IOException("Cannot read selected file");
                    byte[] buffer = new byte[65536];
                    long total = 0;
                    int count;
                    while ((count = in.read(buffer)) != -1) {
                        total += count;
                        if (total > (128L << 20)) throw new java.io.IOException("Jar exceeds the 128 MiB limit");
                        out.write(buffer, 0, count);
                    }
                }
                System.loadLibrary("cryon_client_android");
                importResult = importNative(source.getAbsolutePath(), new File(job, "report").getAbsolutePath(),
                        game.getAbsolutePath(), resources);
            } catch (Throwable failure) {
                org.json.JSONObject error = new org.json.JSONObject();
                try { error.put("error", failure.toString()); } catch (Exception ignored) {}
                importResult = error.toString();
            } finally {
                if (source != null) source.delete();
                importing = false;
            }
        });
        main.post(watchImport);
    }

    private void showReport(String json) {
        try {
            org.json.JSONObject report = new org.json.JSONObject(json);
            StringBuilder text = new StringBuilder();
            if (report.has("error")) text.append(report.getString("error"));
            else {
                text.append(report.optString("summary")).append("\n\n");
                for (String key : new String[] {"ports", "skipped", "depends"}) {
                    org.json.JSONArray entries = report.optJSONArray(key);
                    if (entries == null || entries.length() == 0) continue;
                    text.append(key.toUpperCase(java.util.Locale.ROOT)).append("\n");
                    for (int i = 0; i < entries.length(); i++) text.append("- ").append(entries.optString(i)).append("\n");
                    text.append("\n");
                }
                text.append("New content is disabled until you enable it. Java code never runs. Test partial imports in a separate world. Source licenses still apply.");
            }
            TextView body = new TextView(this);
            body.setText(text);
            body.setTextIsSelectable(true);
            body.setPadding(32, 16, 32, 16);
            android.widget.ScrollView scroll = new android.widget.ScrollView(this);
            scroll.addView(body);
            new AlertDialog.Builder(this).setTitle(report.optString("name", "Import report"))
                    .setView(scroll).setPositiveButton("Close", null).show();
        } catch (Exception error) {
            Toast.makeText(this, "Cannot read import report: " + error.getMessage(), Toast.LENGTH_LONG).show();
        }
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
        items.clear();
        items.addAll(Mods.list(dir(), kind));
        rows.notifyDataSetChanged();
        findViewById(R.id.empty).setVisibility(items.isEmpty() ? View.VISIBLE : View.GONE);
        ((TextView) findViewById(R.id.empty)).setText(items.isEmpty()
                ? "No " + KINDS[kind].toLowerCase(java.util.Locale.ROOT) + " in this instance.\n\n"
                    + (kind == Mods.CONTENT ? "Import a jar above, or copy a content .hmod to:\n" : "Copy a compatible pack to:\n")
                    + new File(dir(), kind == Mods.CONTENT ? "cryon-mods" : kind == Mods.RESOURCE ? "resourcepacks" : "shaderpacks").getAbsolutePath()
                : "");
        ((TextView) findViewById(R.id.count)).setText(plural(items.size(), kind)
                + " in " + instances.get(instance));
    }

    private static String emptyFor(int kind) {
        switch (kind) {
            case Mods.RESOURCE:
                return "No resource packs here.\n\nCopy a .zip into "
                        + "Android/data/dev.cryon.client/files/<instance>/resourcepacks.";
            case Mods.SHADER:
                return "No shader packs here.\n\nCopy a .zip into "
                        + "Android/data/dev.cryon.client/files/<instance>/shaderpacks.";
            default:
                return "No mods here.\n\nA Cryon content mod (.hmod) goes in "
                        + "Android/data/dev.cryon.client/files/<instance>/cryon-mods,"
                        + " and is off until tapped here.";
        }
    }

    private static String plural(int count, int kind) {
        String noun = kind == Mods.RESOURCE ? "resource pack"
                : kind == Mods.SHADER ? "shader pack" : "mod";
        return count + " " + noun + (count == 1 ? "" : "s");
    }

    /** Tap: the one thing the game needs telling, which is whether to load this. */
    private void toggle(Mods.Item item) {
        if (!item.toggleable) {
            ask(item);
            return;
        }
        try {
            Mods.setEnabled(dir(), item, !item.enabled);
            refresh();
        } catch (Exception refused) {
            Toast.makeText(this, "Could not change " + item.title + ": "
                    + refused.getMessage(), Toast.LENGTH_LONG).show();
        }
    }

    private void ask(Mods.Item item) {
        String[] actions = new String[] {"Details / compatibility", "Remove"};
        new AlertDialog.Builder(this)
                .setTitle(item.title)
                .setItems(actions, (dialog, which) -> {
                    if (which == 0) {
                        details(item);
                    } else {
                        confirmDelete(item);
                    }
                })
                .setNegativeButton(R.string.close, null)
                .show();
    }

    private void details(Mods.Item item) {
        File report = new File(item.file, "port-report.json");
        if (report.isFile() && report.length() < 1024 * 1024) {
            try { showReport(new String(java.nio.file.Files.readAllBytes(report.toPath()), java.nio.charset.StandardCharsets.UTF_8)); return; }
            catch (Exception ignored) {}
        }
        StringBuilder text = new StringBuilder();
        Mods.line(text, "File", item.file.getName());
        Mods.line(text, "Id", item.id);
        Mods.line(text, "Version", item.version);
        Mods.line(text, "Size", Mods.size(item));
        Mods.line(text, "Description", item.description);
        new AlertDialog.Builder(this)
                .setTitle(item.title)
                .setMessage(text.toString())
                .setNegativeButton(R.string.close, null)
                .show();
    }

    private void confirmDelete(Mods.Item item) {
        new AlertDialog.Builder(this)
                .setTitle("Remove " + item.title + "?")
                .setMessage("The file (" + Mods.size(item)
                        + ") is deleted from this instance. A mod the game ships"
                        + " with is not affected.")
                .setPositiveButton("Remove", (dialog, which) -> {
                    Cryon.deleteAll(item.file);
                    refresh();
                })
                .setNegativeButton(R.string.close, null)
                .show();
    }

    private void leave() {
        finish();
        overridePendingTransition(R.anim.enter_rise, R.anim.exit_sink);
    }

    private class Rows extends ArrayAdapter<Mods.Item> {
        Rows() {
            super(ModsActivity.this, R.layout.text_row, R.id.row_title, items);
        }

        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            View row = convertView != null ? convertView
                    : LayoutInflater.from(parent.getContext())
                            .inflate(R.layout.text_row, parent, false);
            Mods.Item item = getItem(position);
            ((ImageView) row.findViewById(R.id.row_mark)).setImageResource(
                    kind == Mods.CONTENT ? R.drawable.ic_mods : R.drawable.ic_pack);
            ((TextView) row.findViewById(R.id.row_title)).setText(item.title);
            ((TextView) row.findViewById(R.id.row_detail)).setText(summary(item));
            TextView state = row.findViewById(R.id.row_state);
            state.setText(!item.toggleable ? "" : item.enabled ? "ON" : "OFF");
            state.setTextColor(getColor(item.enabled && item.toggleable
                    ? R.color.shadow_good : R.color.shadow_faint));
            return row;
        }
    }

    /** `1.2 MiB · a building mod`, the two things that tell one file from another. */
    private String summary(Mods.Item item) {
        StringBuilder text = new StringBuilder(Mods.size(item));
        text.append(" · ").append(item.isFolder() ? "folder" : item.file.getName());
        if (!item.description.isEmpty()) {
            text.append("\n").append(item.description);
        }
        return text.toString();
    }
}
