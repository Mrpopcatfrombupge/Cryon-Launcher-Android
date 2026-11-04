package dev.cryon.client;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.graphics.Color;
import android.net.Uri;
import android.os.Bundle;
import android.text.InputType;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.SeekBar;
import android.widget.TextView;
import android.widget.Toast;

import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;

/**
 * The control editor: the game's own control layer, in a screen, with the buttons
 * free to move.
 *
 * It is the same view the game puts over the world, which is the only way what a
 * player arranges is what they play with. A control is dragged with a finger, its
 * corner pulls it bigger, and a tap opens what it does — a key, a mouse button, a
 * stick, held or toggled, shown in the world or in a menu.
 *
 * Layouts are written in the shape every Android launcher of this kind uses, so a
 * layout made elsewhere can be opened here, and one made here can be taken back.
 */
public final class ControlsActivity extends Activity {
    private static final int EXPORT = 4;
    private static final int IMPORT = 5;

    private final ControlLayer.Tuning tuning = new ControlLayer.Tuning();
    private Cryon cryon;
    private Layout layout;
    private ControlLayer layer;
    private String name = "default";
    private TextView title;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        cryon = Cryon.open(this);
        name = cryon.preference("defaultCtrl", "default");
        layout = Layout.load(cryon.files(), name);
        tuning.scale = scale();

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(0xFF0B0B0D);
        root.addView(head());
        layer = new ControlLayer(this, layout, tuning);
        layer.setListener(this::open);
        root.addView(layer, new LinearLayout.LayoutParams(-1, 0, 1f));
        root.addView(bar());
        setContentView(root);
        rename();
    }

    /** The layout's name, and what is in it: the count is the only clue a player
     *  has that a drag took. */
    private View head() {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(Gravity.CENTER_VERTICAL);
        row.setPadding(dp(10), dp(6), dp(6), dp(2));
        title = new TextView(this);
        title.setTextColor(0xFFB9BCC4);
        title.setTextSize(14);
        row.addView(title, new LinearLayout.LayoutParams(0, -2, 1f));
        add(row, "BACK", v -> leave());
        return row;
    }

    /** The row of things the editor can do, along the bottom where a thumb is. */
    private LinearLayout bar() {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setPadding(dp(6), dp(2), dp(6), dp(6));
        add(row, "ADD", v -> add(false));
        add(row, "STICK", v -> add(true));
        add(row, "SAVE", v -> save());
        add(row, "OPEN", v -> choose());
        add(row, "DEFAULT", v -> makeDefault());
        add(row, "EXPORT", v -> export());
        add(row, "IMPORT", v -> importLayout());
        add(row, "RESET", v -> reset());
        return row;
    }

    /** A small flat button in a row, with the label the player reads. */
    private void add(LinearLayout into, String label, View.OnClickListener action) {
        Button button = new Button(this);
        button.setText(label);
        button.setTextSize(11);
        button.setTextColor(Color.rgb(230, 230, 230));
        button.setAllCaps(true);
        button.setOnClickListener(action);
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f);
        into.addView(button, params);
    }

    // --- the controls themselves ---

    /** A new control, where there is room for it. */
    private void add(boolean stick) {
        Layout.Element element = new Layout.Element();
        if (stick) {
            element.name = "Move";
            element.stick = true;
            element.keys = new int[] {Layout.Keys.W, Layout.Keys.A,
                    Layout.Keys.S, Layout.Keys.D};
            element.width = 120;
            element.height = 120;
            element.corner = 100;
            element.opacity = 0.3;
            element.x = "${left} + ${margin} * 2";
            element.y = "${bottom} - ${margin} * 2 - ${height}";
        } else {
            element.name = "New";
            element.keys = new int[] {Layout.Keys.SPACE, 0, 0, 0};
            element.width = 48;
            element.height = 48;
            element.x = "${left} + px(" + dp(60) + ")";
            element.y = "${top} + px(" + dp(60) + ")";
        }
        layout.elements.add(element);
        layer.rebuild();
        rename();
        open(element);
    }

    /** What a control does, in a panel that slides up from the bottom of it. */
    private void open(Layout.Element element) {
        LinearLayout form = new LinearLayout(this);
        form.setOrientation(LinearLayout.VERTICAL);
        form.setPadding(dp(14), dp(6), dp(14), 0);

        EditText label = new EditText(this);
        label.setText(element.name);
        label.setInputType(InputType.TYPE_CLASS_TEXT);
        label.setTextColor(0xFFE6E6E6);
        form.addView(label);

        form.addView(words("Key"));
        Button key = new Button(this);
        key.setText(Layout.Keys.name(element.keys[0]));
        key.setOnClickListener(v -> pick(code -> {
            element.keys[0] = code;
            key.setText(Layout.Keys.name(code));
        }));
        form.addView(key);

        form.addView(words("Second key, if any"));
        Button second = new Button(this);
        second.setText(Layout.Keys.name(element.keys[1]));
        second.setOnClickListener(v -> pick(code -> {
            element.keys[1] = code;
            second.setText(Layout.Keys.name(code));
        }));
        form.addView(second);

        if (element.stick) {
            form.addView(check("Lock forward", element.forwardLock,
                    on -> element.forwardLock = on));
            form.addView(check("Wherever the finger points", element.absolute,
                    on -> element.absolute = on));
        }
        form.addView(check("Stays down after a tap", element.toggle,
                on -> element.toggle = on));
        form.addView(check("A drag across it slides", element.swipeable,
                on -> element.swipeable = on));
        form.addView(check("Show in the world", element.inGame,
                on -> element.inGame = on));
        form.addView(check("Show in a menu", element.inMenu, on -> element.inMenu = on));

        form.addView(words("Width"));
        form.addView(slider(24, 240, (int) element.width,
                value -> element.width = value));
        form.addView(words("Height"));
        form.addView(slider(24, 240, (int) element.height,
                value -> element.height = value));
        form.addView(words("Opacity"));
        form.addView(slider(5, 100, (int) (element.opacity * 100),
                value -> element.opacity = value / 100.0));
        form.addView(words("Roundness"));
        form.addView(slider(0, 100, (int) element.corner, value -> element.corner = value));

        new AlertDialog.Builder(this)
                .setView(form)
                .setPositiveButton("Done", (dialog, which) -> {
                    element.name = label.getText().toString();
                    layer.rebuild();
                })
                .setNeutralButton("Copy", (dialog, which) -> {
                    layout.elements.add(copy(element));
                    layer.rebuild();
                    rename();
                })
                .setNegativeButton("Delete", (dialog, which) -> {
                    layout.elements.remove(element);
                    layer.rebuild();
                    rename();
                })
                .show();
    }

    private interface Codes {
        void chose(int code);
    }

    private interface Values {
        void gave(int value);
    }

    private interface Switches {
        void set(boolean on);
    }

    /** The list of keys a control can be given. */
    private void pick(Codes whenChosen) {
        String[] names = Layout.Keys.PICK;
        new AlertDialog.Builder(this)
                .setTitle("A key")
                .setItems(names, (dialog, which) ->
                        whenChosen.chose(Layout.Keys.code(names[which])))
                .setNeutralButton("None", (dialog, which) -> whenChosen.chose(0))
                .show();
    }

    private TextView words(String text) {
        TextView view = new TextView(this);
        view.setText(text);
        view.setTextColor(0xFF8C8F98);
        view.setTextSize(12);
        view.setPadding(0, dp(8), 0, dp(2));
        return view;
    }

    private SeekBar slider(int from, int to, int at, Values whenSet) {
        SeekBar bar = new SeekBar(this);
        bar.setMax(to - from);
        bar.setProgress(Math.max(0, Math.min(to - from, at - from)));
        bar.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override
            public void onProgressChanged(SeekBar seek, int value, boolean fromUser) {
                whenSet.gave(value + from);
            }

            @Override
            public void onStartTrackingTouch(SeekBar seek) {}

            @Override
            public void onStopTrackingTouch(SeekBar seek) {}
        });
        return bar;
    }

    private CheckBox check(String text, boolean at, Switches whenSet) {
        CheckBox box = new CheckBox(this);
        box.setText(text);
        box.setTextColor(0xFFE6E6E6);
        box.setChecked(at);
        box.setOnCheckedChangeListener((view, on) -> whenSet.set(on));
        return box;
    }

    private Layout.Element copy(Layout.Element element) {
        Layout.Element other = new Layout.Element();
        other.name = element.name;
        other.keys = element.keys.clone();
        other.x = element.x;
        other.y = element.y;
        other.width = element.width;
        other.height = element.height;
        other.opacity = element.opacity;
        other.corner = element.corner;
        other.colour = element.colour;
        other.stroke = element.stroke;
        other.strokeWidth = element.strokeWidth;
        other.toggle = element.toggle;
        other.swipeable = element.swipeable;
        other.inGame = element.inGame;
        other.inMenu = element.inMenu;
        other.stick = element.stick;
        other.forwardLock = element.forwardLock;
        other.absolute = element.absolute;
        return other;
    }

    // --- layouts ---

    private void rename() {
        title.setText(name + "  ·  " + layout.elements.size() + " controls");
    }

    private void save() {
        try {
            layout.scaledAt = tuning.scale;
            layout.save(cryon.files(), name);
            toast("Saved " + name);
        } catch (Exception failure) {
            toast("could not save: " + failure);
        }
    }

    /** The layouts on the device, to start editing another one. */
    private void choose() {
        List<String> found = new ArrayList<>();
        found.add("default");
        java.io.File[] saved = Layout.folder(cryon.files())
                .listFiles((dir, file) -> file.endsWith(".json"));
        if (saved != null) {
            for (java.io.File file : saved) {
                String stem = file.getName()
                        .substring(0, file.getName().length() - 5);
                if (!found.contains(stem)) {
                    found.add(stem);
                }
            }
        }
        String[] names = found.toArray(new String[0]);
        new AlertDialog.Builder(this)
                .setTitle("A layout")
                .setItems(names, (dialog, which) -> {
                    name = names[which];
                    layout = Layout.load(cryon.files(), name);
                    layer = replace(layer, layout);
                    rename();
                })
                .show();
    }

    /** Put a new layer in the same place as the old one. */
    private ControlLayer replace(ControlLayer old, Layout fresh) {
        ControlLayer next = new ControlLayer(this, fresh, tuning);
        next.setListener(this::open);
        LinearLayout parent = (LinearLayout) old.getParent();
        parent.addView(next, parent.indexOfChild(old));
        parent.removeView(old);
        layer = next;
        return next;
    }

    private void makeDefault() {
        try {
            layout.scaledAt = tuning.scale;
            layout.save(cryon.files(), name);
            cryon.setPreference("defaultCtrl", name);
            toast(name + " is what the game starts with");
        } catch (Exception failure) {
            toast("could not save: " + failure);
        }
    }

    private void reset() {
        new AlertDialog.Builder(this)
                .setTitle("Start again")
                .setMessage("This layout is replaced with the controls the game "
                        + "gives a new player.")
                .setPositiveButton("Replace", (dialog, which) -> {
                    layout = Layout.defaults();
                    replace(layer, layout);
                    rename();
                })
                .setNegativeButton("Keep", null)
                .show();
    }

    private void export() {
        Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("application/json");
        intent.putExtra(Intent.EXTRA_TITLE, name + ".json");
        startActivityForResult(intent, EXPORT);
    }

    private void importLayout() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("*/*");
        startActivityForResult(intent, IMPORT);
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (result != RESULT_OK || data == null || data.getData() == null) {
            return;
        }
        Uri uri = data.getData();
        try {
            if (request == EXPORT) {
                writeOut(uri, Layout.write(layout));
                toast("Written out");
            } else {
                String text = readIn(uri);
                Layout read = Layout.read(text);
                layout = read;
                replace(layer, read);
                rename();
                toast(read.elements.size() + " controls read");
            }
        } catch (Exception failure) {
            toast("could not " + (request == EXPORT ? "write" : "read") + ": " + failure);
        }
    }

    private void writeOut(Uri uri, String text) throws Exception {
        try (OutputStream out = getContentResolver().openOutputStream(uri)) {
            if (out != null) {
                out.write(text.getBytes("UTF-8"));
            }
        }
    }

    private String readIn(Uri uri) throws Exception {
        try (InputStream in = getContentResolver().openInputStream(uri)) {
            if (in == null) {
                throw new IllegalStateException("nothing to read");
            }
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
            byte[] buffer = new byte[8192];
            for (int read = in.read(buffer); read > 0; read = in.read(buffer)) {
                bytes.write(buffer, 0, read);
            }
            return new String(bytes.toByteArray(), "UTF-8");
        }
    }

    private void leave() {
        finish();
    }

    private int dp(float value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    /** The button size the settings screen set, which the game reads as well. */
    private double scale() {
        String text = Cryon.readLines(cryon.envFile(), '=').get("CRYON_BUTTON_SCALE");
        try {
            return text == null ? 100 : Double.parseDouble(text.trim());
        } catch (NumberFormatException nonsense) {
            return 100;
        }
    }

    private void toast(String text) {
        Toast.makeText(this, text, Toast.LENGTH_SHORT).show();
    }
}
