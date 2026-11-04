package dev.cryon.client;

import android.app.Activity;
import android.app.AlertDialog;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.text.Editable;
import android.text.TextWatcher;
import android.view.LayoutInflater;
import android.view.View;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.SeekBar;
import android.widget.TextView;
import android.widget.Toast;
import java.io.IOException;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;

/** One searchable settings screen. Display units and file formats live in Settings. */
public class SettingsActivity extends Activity {
    public static final String LAUNCHER = "launcher";
    private Cryon cryon;
    private LinearLayout tabs;
    private LinearLayout rows;
    private EditText search;
    private int selected;
    private Map<String, Double> values = new LinkedHashMap<>();
    private final Map<Settings.Option, Double> pending = new LinkedHashMap<>();
    private final Handler main = new Handler(Looper.getMainLooper());
    private final Runnable flush = this::flushPending;

    @Override protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        setContentView(R.layout.settings);
        cryon = Cryon.open(this);
        ((TextView) findViewById(R.id.title)).setText("Settings");
        findViewById(R.id.back).setOnClickListener(v -> finish());
        tabs = findViewById(R.id.tabs);
        rows = findViewById(R.id.rows);
        search = findViewById(R.id.settings_search);
        selected = saved == null ? (getIntent().getBooleanExtra(LAUNCHER, false) ? 3 : 0)
                : saved.getInt("tab", 0);
        selected = Math.max(0, Math.min(Settings.TABS.length - 1, selected));
        for (int i = 0; i < Settings.TABS.length; i++) {
            final int index = i;
            Button tab = new Button(this);
            tab.setText(Settings.TABS[i]);
            tab.setAllCaps(false);
            tab.setMinHeight(dp(48));
            tab.setOnClickListener(v -> {
                flushPending();
                selected = index;
                search.setText("");
                rebuild();
            });
            tabs.addView(tab);
        }
        if (saved != null) search.setText(saved.getString("search", ""));
        search.addTextChangedListener(new TextWatcher() {
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {}
            public void onTextChanged(CharSequence s, int start, int before, int count) {
                flushPending();
                rebuild();
            }
            public void afterTextChanged(Editable e) {}
        });
        rebuild();
    }

    @Override protected void onSaveInstanceState(Bundle out) {
        out.putInt("tab", selected);
        out.putString("search", search.getText().toString());
        super.onSaveInstanceState(out);
    }

    private void rebuild() {
        values = Settings.snapshot(cryon);
        rows.removeAllViews();
        for (int i = 0; i < tabs.getChildCount(); i++) {
            ((TextView) tabs.getChildAt(i)).setTextColor(getColor(i == selected
                    ? R.color.shadow_accent : R.color.shadow_mist));
            tabs.getChildAt(i).setSelected(i == selected);
        }
        String query = search.getText().toString().trim().toLowerCase(Locale.ROOT);
        if (query.isEmpty() && (selected == 0 || selected == 3)) {
            rows.addView(note("QUICK PRESETS  ·  changes save automatically"));
            LinearLayout presets = new LinearLayout(this);
            for (String name : new String[] {"Mobile", "Balanced", "Quality"}) {
                Button button = new Button(this);
                button.setText(name);
                button.setTextSize(12);
                button.setAllCaps(false);
                presets.addView(button, new LinearLayout.LayoutParams(0, dp(48), 1));
                button.setOnClickListener(v -> new AlertDialog.Builder(this)
                        .setTitle("Apply " + name + " preset?")
                        .setMessage("Replaces video quality and streaming limits in this instance. Renderer, controls, mods and account stay unchanged.")
                        .setPositiveButton("Apply", (d, which) -> {
                            flushPending();
                            try { Settings.applyPreset(cryon, name); rebuild(); }
                            catch (IOException e) { error(e); }
                        }).setNegativeButton("Cancel", null).show());
            }
            rows.addView(presets);
        }
        int count = 0;
        for (Settings.Option option : Settings.ALL) {
            String haystack = (option.label + " " + option.key + " " + option.group + " "
                    + (option.note == null ? "" : option.note)).toLowerCase(Locale.ROOT);
            if (query.isEmpty() ? !option.group.equals(Settings.TABS[selected]) : !haystack.contains(query)) continue;
            boolean supported = Settings.supported(values, option);
            View row = option.isChoice() ? choiceRow(option, supported) : sliderRow(option, supported);
            rows.addView(row);
            if (!supported) rows.addView(note("Not supported by OpenGL ES. Select Vulkan and restart to use this feature."));
            else if (option.note != null) rows.addView(note(option.note));
            count++;
        }
        if (count == 0) rows.addView(note("No matching settings."));
        rows.addView(note("Changes save here. Restart the game for renderer, worker, sound and device options. In-game options can override this instance's values."));
    }

    private View sliderRow(Settings.Option option, boolean enabled) {
        LinearLayout row = (LinearLayout) LayoutInflater.from(this).inflate(R.layout.row_slider, rows, false);
        TextView label = row.findViewById(R.id.row_name);
        TextView value = row.findViewById(R.id.row_value);
        SeekBar seek = row.findViewById(R.id.row_seek);
        label.setText(option.label);
        seek.setContentDescription(option.label);
        seek.setMax((int) Math.round((option.to - option.from) / option.step));
        double at = values.get(option.key);
        seek.setProgress((int) Math.round((at - option.from) / option.step));
        value.setText(shown(at, option));
        seek.setEnabled(enabled);
        seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            public void onProgressChanged(SeekBar bar, int progress, boolean fromUser) {
                double next = option.normalize(option.from + progress * option.step);
                value.setText(shown(next, option));
                if (fromUser) {
                    pending.put(option, next);
                    main.removeCallbacks(flush);
                    main.postDelayed(flush, 250);
                }
            }
            public void onStartTrackingTouch(SeekBar bar) {}
            public void onStopTrackingTouch(SeekBar bar) { flushPending(); }
        });
        return row;
    }

    private View choiceRow(Settings.Option option, boolean enabled) {
        LinearLayout row = (LinearLayout) LayoutInflater.from(this).inflate(R.layout.row_choice, rows, false);
        ((TextView) row.findViewById(R.id.row_name)).setText(option.label);
        Button button = row.findViewById(R.id.row_choice);
        button.setText(option.labels[(int) Math.round(values.get(option.key))]);
        button.setContentDescription(option.label + ": " + button.getText());
        button.setEnabled(enabled);
        button.setOnClickListener(v -> new AlertDialog.Builder(this).setTitle(option.label)
                .setSingleChoiceItems(option.labels, (int) Math.round(Settings.value(cryon, option)), (dialog, which) -> {
                    try {
                        Settings.put(cryon, option, which);
                        dialog.dismiss();
                        rebuild();
                    } catch (IOException e) { error(e); }
                }).setNegativeButton("Cancel", null).show());
        return row;
    }

    private void flushPending() {
        main.removeCallbacks(flush);
        boolean failed = false;
        for (Map.Entry<Settings.Option, Double> value : pending.entrySet()) {
            try { Settings.put(cryon, value.getKey(), value.getValue()); }
            catch (IOException e) { error(e); failed = true; }
        }
        pending.clear();
        if (failed && !isFinishing()) rebuild();
    }

    @Override protected void onPause() { flushPending(); super.onPause(); }
    @Override protected void onDestroy() { main.removeCallbacksAndMessages(null); super.onDestroy(); }

    private TextView note(String text) {
        TextView view = new TextView(this);
        view.setText(text);
        view.setTextSize(12);
        view.setTextColor(getColor(R.color.shadow_mist));
        view.setPadding(0, dp(6), 0, dp(12));
        return view;
    }

    private String shown(double value, Settings.Option option) {
        if ((option.key.equals("workers") || option.key.equals("guiScale")) && value == 0) return "Auto";
        if (option.key.equals("maxFps") && value == 260) return "Unlimited";
        String text = option.step >= 1 ? Integer.toString((int) Math.round(value))
                : String.format(Locale.US, "%.2f", value);
        switch (option.key) {
            case "renderDistance": case "simulationDistance": return text + " chunks";
            case "maxFps": return text + " fps";
            case "resolutionScale": case "launcherTextScale": case "CRYON_BUTTON_SCALE": return text + "%";
            case "uploadBudgetKiB": return text + " KiB";
            case "CRYON_LONG_PRESS": return text + " ms";
            case "entityDistanceScaling": return text + "x";
            default: return text;
        }
    }
    private int dp(int value) { return Math.round(value * getResources().getDisplayMetrics().density); }
    private void error(Exception e) { Toast.makeText(this, "Not saved: " + e.getMessage(), Toast.LENGTH_LONG).show(); }
}
