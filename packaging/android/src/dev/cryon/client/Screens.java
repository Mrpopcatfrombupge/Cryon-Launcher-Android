package dev.cryon.client;

import android.app.Activity;
import android.view.LayoutInflater;
import android.view.View;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.util.List;

/**
 * The rows of choices the list screens share.
 *
 * Every one of them is asked twice: which instance, and which kind of thing inside
 * it. The shape is the same — words that fit on two thumbs, the chosen one rimmed —
 * so it is written once.
 */
final class Screens {
    /**
     * Fill a strip with `names`, marking `chosen`, calling `onPick` with the index
     * of any tap. Rebuilding the whole row on each pick is what the emulator's widget
     * pool expects, and the lists are four or five words long.
     */
    static void strip(final Activity screen, LinearLayout strip, List<String> names,
            int chosen, final Callback onPick) {
        strip.removeAllViews();
        LayoutInflater inflater = LayoutInflater.from(screen);
        for (int i = 0; i < names.size(); i++) {
            View chip = inflater.inflate(R.layout.instance_chip, strip, false);
            TextView label = chip.findViewById(R.id.instance_label);
            label.setText(names.get(i));
            boolean selected = i == chosen;
            chip.setBackgroundResource(selected ? R.drawable.chip_on : R.drawable.chip);
            label.setTextColor(screen.getColor(selected
                    ? R.color.shadow_accent : R.color.shadow_mist));
            final int index = i;
            chip.setOnClickListener(v -> onPick.pick(index));
            strip.addView(chip);
        }
    }

    interface Callback {
        void pick(int index);
    }

    private Screens() {
    }
}
