package dev.cryon.client;

import android.app.Activity;
import android.app.AlertDialog;
import android.os.Bundle;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.ArrayAdapter;
import android.widget.EditText;
import android.widget.ListView;
import android.widget.TextView;
import android.widget.Toast;
import java.io.File;
import java.util.List;

/**
 * The instances on this device.
 *
 * One row each, the running one marked, tap to switch, hold for the rest. An
 * instance is a folder the game writes into, so creating one is making a folder and
 * deleting one is removing it with everything inside — which is why deleting asks
 * first, and why the default cannot be deleted at all.
 */
public class InstancesActivity extends Activity {
    private File files;
    private ListView list;
    private List<String> names;
    private Rows rows;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        setContentView(R.layout.instances);
        ((TextView) findViewById(R.id.title)).setText(R.string.menu_instances);
        findViewById(R.id.back).setOnClickListener(v -> leave());
        files = Cryon.open(this).files();

        list = findViewById(R.id.list);
        names = Instances.list(files);
        rows = new Rows();
        list.setAdapter(rows);
        list.setOnItemClickListener((parent, view, position, id) -> use(names.get(position)));
        list.setOnItemLongClickListener((parent, view, position, id) -> {
            offer(names.get(position));
            return true;
        });

        findViewById(R.id.new_instance).setOnClickListener(v -> create());
    }

    /** The instance PLAY will use. */
    private void use(String name) {
        try {
            Instances.setCurrent(files, name);
            refresh();
            Toast.makeText(this, name + " will run next", Toast.LENGTH_SHORT).show();
        } catch (Exception refused) {
            Toast.makeText(this, "Could not switch: " + refused.getMessage(),
                    Toast.LENGTH_SHORT).show();
        }
    }

    private void create() {
        EditText field = new EditText(this);
        field.setHint("Name");
        field.setSingleLine(true);
        new AlertDialog.Builder(this)
                .setTitle("New instance")
                .setView(field)
                .setPositiveButton("Create", (dialog, which) -> {
                    String name = field.getText().toString();
                    if (!Instances.create(files, name)) {
                        Toast.makeText(this,
                                "\"" + name + "\" already exists or cannot be a folder name",
                                Toast.LENGTH_LONG).show();
                        return;
                    }
                    refresh();
                })
                .setNegativeButton("Cancel", null)
                .show();
    }

    /** Hold a row: the things you do to an instance you are not using yet. */
    private void offer(String name) {
        boolean current = Instances.current(files).equals(name);
        String[] actions = current
                ? new String[] {"Rename"}
                : new String[] {"Use this instance", "Rename", "Delete"};
        new AlertDialog.Builder(this)
                .setTitle(name)
                .setItems(actions, (dialog, which) -> {
                    if (current) {
                        if (which == 0) {
                            rename(name);
                        }
                        return;
                    }
                    switch (which) {
                        case 0:
                            use(name);
                            break;
                        case 1:
                            rename(name);
                            break;
                        default:
                            confirmDelete(name);
                            break;
                    }
                })
                .setNegativeButton(R.string.close, null)
                .show();
    }

    private void rename(String from) {
        EditText field = new EditText(this);
        field.setText(from);
        field.setSingleLine(true);
        new AlertDialog.Builder(this)
                .setTitle("Rename instance")
                .setView(field)
                .setPositiveButton("Rename", (dialog, which) -> {
                    String to = field.getText().toString();
                    if (!Instances.rename(files, from, to)) {
                        Toast.makeText(this, "\"" + to + "\" is not available",
                                Toast.LENGTH_LONG).show();
                        return;
                    }
                    refresh();
                })
                .setNegativeButton(R.string.close, null)
                .show();
    }

    /**
     * Deleting an instance takes its worlds, its settings and its mods with it, so
     * it asks in words rather than with a swipe.
     */
    private void confirmDelete(String name) {
        new AlertDialog.Builder(this)
                .setTitle("Delete " + name + "?")
                .setMessage("Its worlds, settings, resource packs and mods are deleted too.")
                .setPositiveButton("Delete", (dialog, which) -> {
                    if (!Instances.delete(files, name)) {
                        Toast.makeText(this, "Could not delete " + name,
                                Toast.LENGTH_LONG).show();
                        return;
                    }
                    refresh();
                })
                .setNegativeButton(R.string.close, null)
                .show();
    }

    private void refresh() {
        names.clear();
        names.addAll(Instances.list(files));
        rows.notifyDataSetChanged();
    }

    /** Back the way it came: this screen sinks, the one behind it rises. */
    private void leave() {
        finish();
        overridePendingTransition(R.anim.enter_rise, R.anim.exit_sink);
    }

    /** The rows, with the running one marked beside its name. */
    private class Rows extends ArrayAdapter<String> {
        Rows() {
            super(InstancesActivity.this, R.layout.instance_row, R.id.instance_name, names);
        }

        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            View row = convertView != null ? convertView
                    : LayoutInflater.from(parent.getContext())
                            .inflate(R.layout.instance_row, parent, false);
            String name = getItem(position);
            ((TextView) row.findViewById(R.id.instance_name)).setText(name);
            TextView state = row.findViewById(R.id.instance_state);
            boolean running = Instances.current(files).equals(name);
            state.setText(running ? "IN USE" : "");
            return row;
        }
    }
}
