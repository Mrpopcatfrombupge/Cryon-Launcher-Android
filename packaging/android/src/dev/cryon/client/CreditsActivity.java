package dev.cryon.client;

import android.app.Activity;
import android.os.Bundle;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.ArrayAdapter;
import android.widget.ImageView;
import android.widget.ListView;
import android.widget.TextView;
import java.util.ArrayList;
import java.util.List;

/**
 * Who made the things this app is built out of.
 *
 * Not a formality. The engine is a port of somebody else's work, the renderer is
 * somebody else's, the shaders are AMD's, the font and the icons are three other
 * projects' licenses, and the layer that draws these very rows follows Pojav's
 * design — and one of those is a license that asks for attribution in return for
 * letting the code be used. A screen that is easy to reach and hard to misquote is
 * the right place to say so; the exact texts are in the repository's
 * `THIRD_PARTY_NOTICES.md`.
 *
 * The words here are the ones the notices file uses, because a summary that drifts
 * from the license it is summarising helps nobody.
 */
public class CreditsActivity extends Activity {
    /** A project, and what it contributed. */
    private static final class Credit {
        final int mark;
        final String title;
        final String body;

        Credit(int mark, String title, String body) {
            this.mark = mark;
            this.title = title;
            this.body = body;
        }
    }

    private static final Credit[] CREDITS = {
        new Credit(R.drawable.logo_cryon, "Cryon",
                "This launcher and the game it starts: Minecraft 26.3 in Rust, with its own "
                        + "renderer, world storage, mods and touch controls."),
        new Credit(R.drawable.ic_credits, "Horizon",
                "The engine this is ported from. Its renderer, level storage and mod format "
                        + "are Horizon's design, carried here rather than reinvented."),
        new Credit(R.drawable.ic_worlds, "Minecraft: Java Edition",
                "Mojang Studios. Cryon is not their code and is not endorsed by them or by "
                        + "Microsoft. It loads the assets of a copy you own and signs in "
                        + "through your own Microsoft account."),
        new Credit(R.drawable.ic_settings, "wgpu and naga",
                "The gfx-rs developers, MIT or Apache-2.0. The Vulkan, DirectX and OpenGL "
                        + "ES renderer underneath the game, kept in third_party with "
                        + "upstream fixes applied."),
        new Credit(R.drawable.ic_settings, "AMD FidelityFX",
                "Advanced Micro Devices, MIT. The optical flow and frame generation of FSR 3.1, "
                        + "ported to WGSL. Cryon is not affiliated with AMD."),
        new Credit(R.drawable.ic_mods, "Pixelify Sans",
                "The Pixelify Sans Project Authors, SIL Open Font License 1.1. The type the "
                        + "game's interface is drawn with."),
        new Credit(R.drawable.ic_mods, "Pixelarticons",
                "Gerrit Halfmann, MIT. The interface icons of the game's own theme."),
        new Credit(R.drawable.ic_controls, "PojavLauncher",
                "The Android launcher this app's shape follows: its metrics, its control editor "
                        + "and its in-game overlay. Their own code is LGPL-3 and is not linked "
                        + "here; the design is theirs and the imitation is deliberate."),
    };

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        setContentView(R.layout.screen_list);
        findViewById(R.id.instance_row).setVisibility(View.GONE);
        findViewById(R.id.kind_row).setVisibility(View.GONE);
        ((TextView) findViewById(R.id.title)).setText(R.string.menu_credits);
        findViewById(R.id.back).setOnClickListener(v -> leave());

        List<Credit> credits = new ArrayList<>();
        for (Credit credit : CREDITS) {
            credits.add(credit);
        }
        ListView list = findViewById(R.id.list);
        list.setAdapter(new Rows(credits));
        list.setOnItemClickListener((parent, view, position, id) -> { });
        ((TextView) findViewById(R.id.count)).setText(about());
    }

    /** What this build is, in the two numbers that identify it. */
    private String about() {
        Cryon.Version version = Cryon.open(this).chosen();
        return "Cryon 0.1.0  ·  Minecraft 26.3"
                + (version == null ? "  ·  no version installed" : "  ·  " + version.label);
    }

    private void leave() {
        finish();
        overridePendingTransition(R.anim.enter_rise, R.anim.exit_sink);
    }

    /**
     * The rows are taller than the launcher's usual ones: a credit is two sentences,
     * and cutting them to one line to keep a uniform list would hide the thing the
     * screen exists to say.
     */
    private class Rows extends ArrayAdapter<Credit> {
        Rows(List<Credit> credits) {
            super(CreditsActivity.this, R.layout.credit_row, R.id.credit_title, credits);
        }

        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            View row = convertView != null ? convertView
                    : LayoutInflater.from(parent.getContext())
                            .inflate(R.layout.credit_row, parent, false);
            Credit credit = getItem(position);
            ((ImageView) row.findViewById(R.id.credit_mark)).setImageResource(credit.mark);
            ((TextView) row.findViewById(R.id.credit_title)).setText(credit.title);
            ((TextView) row.findViewById(R.id.credit_body)).setText(credit.body);
            return row;
        }
    }
}
