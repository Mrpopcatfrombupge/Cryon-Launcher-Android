package dev.cryon.client;

import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * The multiplayer list of one instance, as the game keeps it in `servers.dat`.
 *
 * Read here, and only read. The game owns that file: it pings the servers in it,
 * remembers which ones the player accepted a code of conduct on, and rewrites the
 * whole thing whenever a server is added in its own menu. A launcher that wrote it
 * would be a second author of a file the game expects to be the only one holding, so
 * the adding and the deleting stays in the game and the launcher does the thing it
 * can do better — show the list, and start the game pointed at one of them.
 */
public final class Servers {
    public static final class Server {
        public String name;
        public String address;
        /** Whether the entry looks the way an address should; a typo is worth showing. */
        public boolean suspect;
    }

    public static List<Server> list(File gameDir) {
        List<Server> found = new ArrayList<>();
        File data = new File(gameDir, "servers.dat");
        if (!data.isFile()) {
            return found;
        }
        try {
            Map<String, Object> root = Nbt.read(data);
            for (Object entry : Nbt.asList(root.get("servers"))) {
                Map<String, Object> server = Nbt.asCompound(entry);
                Server item = new Server();
                item.name = Nbt.text(server.get("name"), "Unnamed server");
                // Vanilla renamed this tag from `ip` to `address`; both appear in
                // files copied between versions.
                item.address = Nbt.text(server.get("ip"),
                        Nbt.text(server.get("address"), ""));
                if (item.address.isEmpty()) {
                    continue;
                }
                item.suspect = !item.address.matches("[A-Za-z0-9._\\-]+(:\\d{1,5})?");
                found.add(item);
            }
        } catch (Exception unreadable) {
            // A list that cannot be read shows as empty, which is what it is to the
            // launcher; the game still has its own file.
        }
        return found;
    }

    private Servers() {
    }
}
