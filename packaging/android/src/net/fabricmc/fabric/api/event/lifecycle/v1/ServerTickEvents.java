package net.fabricmc.fabric.api.event.lifecycle.v1;

import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * The server-side tick events, in the shape mods already register against.
 *
 * The real Fabric API routes these through its own event machinery; here the
 * registry is the list itself, and the engine's tick hook walks it through
 * {@code dev.cryon.client.fabricapi.Bridge}. A listener costs nothing while
 * the list is empty.
 */
public final class ServerTickEvents {
    /** The world finished a tick. The argument is the tick counter. */
    public interface EndTick {
        void onEndTick(int ticks);
    }

    private static final List<EndTick> END = new CopyOnWriteArrayList<>();

    public static void registerEndTick(EndTick listener) {
        END.add(listener);
    }

    /** Called by the engine bridge; the count is this tick's number. */
    public static void fireEndTick(int ticks) {
        for (EndTick listener : END) {
            try {
                listener.onEndTick(ticks);
            } catch (Throwable failure) {
                android.util.Log.e("CryonBehavior", "tick listener failed: " + failure);
            }
        }
    }

    private ServerTickEvents() {
    }
}
