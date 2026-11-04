package dev.cryon.client.fabricapi;

/**
 * The seam between the Rust engine and the shim's event lists.
 *
 * The engine's native tick hook calls {@link #tick} through JNI — the same
 * path the orientation handler already walks in the other direction. One
 * static method, no state: everything a call needs is in the shim's own
 * registries, and a call with no listeners returns after one check.
 */
public final class Bridge {
    /** Called by the engine, once per world tick, with the tick counter. */
    public static void tick(int ticks) {
        net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents.fireEndTick(ticks);
    }

    private Bridge() {
    }
}
