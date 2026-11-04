package dev.cryon.client.pilot;

import android.util.Log;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents;

/**
 * The behaviour runtime's self-test, written as an ordinary mod.
 *
 * It implements the same interface a Fabric mod does and registers against the
 * same event the shim exposes, so every step of the chain it exercises —
 * initialisation, event registration, the engine's tick reaching Java — is the
 * step a real mod takes. It speaks to logcat, and to nothing else, until the
 * engine side of the bridge is wired.
 */
public class PilotMod implements ModInitializer {
    private static final String TAG = "CryonBehavior";

    @Override
    public void onInitialize() {
        Log.i(TAG, "pilot: onInitialize ran on "
                + Thread.currentThread().getName());
        ServerTickEvents.registerEndTick(ticks -> {
            // Once a minute of game time, so the log proves ticks are flowing
            // without drowning the buffer.
            if (ticks % 600 == 0) {
                Log.i(TAG, "pilot: tick " + ticks + " reached Java");
            }
        });
        Log.i(TAG, "pilot: tick listener registered");
    }
}
