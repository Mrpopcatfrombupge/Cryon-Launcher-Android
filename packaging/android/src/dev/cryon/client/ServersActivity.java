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
import java.util.List;

/**
 * The multiplayer servers the game already knows about.
 *
 * The list is the game's — it is written by its own Multiplayer screen, along with
 * the pings and the code-of-conduct answers the launcher has no business touching —
 * and this screen does the one thing a launcher is better at: starting the game
 * already joined, without four menus in between.
 *
 * So the row is a shortcut, and the note under the list says where the rest happens.
 */
public class ServersActivity extends Activity {
    private Cryon cryon;
    private File files;
    private int instance = 0;
    private List<String> instances = new ArrayList<>();
    private final List<Servers.Server> servers = new ArrayList<>();
    private Rows rows;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        setContentView(R.layout.screen_list);
        ((TextView) findViewById(R.id.title)).setText(R.string.menu_servers);
        ((TextView) findViewById(R.id.empty)).setText(R.string.empty_servers);
        findViewById(R.id.back).setOnClickListener(v -> leave());
        cryon = Cryon.open(this);
        files = cryon.files();

        ListView list = findViewById(R.id.list);
        rows = new Rows();
        list.setAdapter(rows);
        list.setOnItemClickListener((parent, view, position, id) -> join(servers.get(position)));
    }

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

    private void refresh() {
        servers.clear();
        servers.addAll(Servers.list(Instances.dir(files, instances.get(instance))));
        rows.notifyDataSetChanged();
        findViewById(R.id.empty).setVisibility(servers.isEmpty() ? View.VISIBLE : View.GONE);
        ((TextView) findViewById(R.id.count)).setText(
                WorldsActivity.plural(servers.size(), "server")
                        + " in " + instances.get(instance));
    }

    private void join(Servers.Server server) {
        try {
            cryon.launchServer(this, instances.get(instance), server.address);
        } catch (Exception refused) {
            Toast.makeText(this, refused.getMessage(), Toast.LENGTH_LONG).show();
        }
    }

    private void leave() {
        finish();
        overridePendingTransition(R.anim.enter_rise, R.anim.exit_sink);
    }

    private class Rows extends ArrayAdapter<Servers.Server> {
        Rows() {
            super(ServersActivity.this, R.layout.text_row, R.id.row_title, servers);
        }

        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            View row = convertView != null ? convertView
                    : LayoutInflater.from(parent.getContext())
                            .inflate(R.layout.text_row, parent, false);
            Servers.Server server = getItem(position);
            ((ImageView) row.findViewById(R.id.row_mark))
                    .setImageResource(R.drawable.ic_servers);
            ((TextView) row.findViewById(R.id.row_title)).setText(server.name);
            ((TextView) row.findViewById(R.id.row_detail)).setText(server.address);
            ((TextView) row.findViewById(R.id.row_state)).setText(server.suspect ? "?" : "");
            return row;
        }
    }
}
