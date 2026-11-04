package net.fabricmc.api;

/**
 * The one interface nearly every Fabric mod implements.
 *
 * This is the shim's reimplementation, not a copy: the method is the same one
 * mods already declare, and when the behaviour runtime loads a mod it calls
 * this where Fabric Loader would.
 */
public interface ModInitializer {
    void onInitialize();
}
