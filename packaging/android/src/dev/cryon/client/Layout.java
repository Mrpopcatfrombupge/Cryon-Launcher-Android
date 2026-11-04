package dev.cryon.client;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.util.ArrayList;
import java.util.List;

/**
 * A control layout, in the shape every other Android launcher writes one.
 *
 * A layout is a list of elements — buttons, a movement stick, drawers of
 * buttons — and each element says where it stands with an expression rather than
 * a number: `${right} - ${margin} * 2 - ${width}` is "in the bottom-right corner,
 * one margin from the edge", and it still means that on a phone twice the width
 * of another. Sizes are dp, so a button is the size a thumb expects whatever the
 * screen's pixels are.
 *
 * Keeping that format is deliberate. A player who already laid out controls for
 * another launcher can share the folder and keep their layout; the file the
 * editor saves is the file the game plays with, and the file either launcher
 * reads.
 */
public final class Layout {
    /** The format this writes, and the lowest one it still reads. */
    public static final int VERSION = 8;

    /** One control: a button, a stick or a drawer. */
    public static final class Element {
        /** What the player named it, and what the button shows. */
        public String name = "";
        /** GLFW key numbers, up to four at once: W and a mouse button together. */
        public int[] keys = new int[4];
        /** Where it stands, as expressions in the screen's own units. */
        public String x = "${left} + ${margin}";
        public String y = "${bottom} - ${margin} - ${height}";
        /** Its size, in dp. */
        public double width = 48;
        public double height = 48;
        /** 0 is invisible, 1 is solid. Thirty per cent is the plate everyone uses. */
        public double opacity = 0x4D / 255.0;
        /** 0 is square, 100 is a disc. */
        public double corner = 0;
        public int colour = 0x000000;
        public int stroke = 0xFFFFFFFF;
        public double strokeWidth = 0;
        /** Stay down after a tap, rather than only while it is held. */
        public boolean toggle;
        /** A drag across it presses the next one, so a row reads as a wheel. */
        public boolean swipeable;
        /** Shown while the world has the pointer, and while a screen has it. */
        public boolean inGame = true;
        public boolean inMenu = true;
        /** A stick: eight directions, or anywhere the finger points. */
        public boolean stick;
        public boolean forwardLock;
        public boolean absolute;
        /** A drawer: the buttons it opens into, and which way they go. */
        public List<Element> sub = new ArrayList<>();
        public String direction = "UP";
    }
    public int version = VERSION;
    /** The button size the layout was saved at, so a later change rescales it. */
    public double scaledAt = 100;
    public final List<Element> elements = new ArrayList<>();

    /** The folder the layouts live in, in the game's own files. */
    public static File folder(File files) {
        return new File(files, "controls");
    }

    /** The layout the player chose, or the built-in one when nothing was saved. */
    public static Layout load(File files, String name) {
        File file = new File(folder(files), name + ".json");
        if (file.isFile()) {
            try {
                return read(new String(java.nio.file.Files.readAllBytes(file.toPath()),
                        "UTF-8"));
            } catch (Exception ignored) {
                // A layout that will not parse is no reason not to play: the
                // default is always there, and the file stays for whoever broke it.
            }
        }
        return defaults();
    }

    public void save(File files, String name) throws Exception {
        File file = new File(folder(files), name + ".json");
        if (file.getParentFile() != null) {
            file.getParentFile().mkdirs();
        }
        Cryon.write(file, write(this));
    }

    /** Read Pojav's `version`/`mControlDataList` shape, field for field. */
    public static Layout read(String text) throws Exception {
        Layout layout = new Layout();
        JSONObject root = new JSONObject(text);
        layout.version = root.optInt("version", VERSION);
        layout.scaledAt = root.optDouble("scaledAt", 100);
        add(layout.elements, root.optJSONArray("mControlDataList"), false);
        add(layout.elements, root.optJSONArray("mJoystickDataList"), true);
        JSONArray drawers = root.optJSONArray("mDrawerDataList");
        if (drawers != null) {
            for (int i = 0; i < drawers.length(); i++) {
                JSONObject drawer = drawers.optJSONObject(i);
                if (drawer == null) {
                    continue;
                }
                Element element = one(drawer, false);
                element.direction = drawer.optString("orientation", "UP");
                element.sub = new ArrayList<>();
                add(element.sub, drawer.optJSONArray("buttonProperties"), false);
                layout.elements.add(element);
            }
        }
        return layout;
    }

    private static void add(List<Element> into, JSONArray list, boolean stick) {
        if (list == null) {
            return;
        }
        for (int i = 0; i < list.length(); i++) {
            JSONObject object = list.optJSONObject(i);
            if (object != null) {
                into.add(one(object, stick));
            }
        }
    }

    private static Element one(JSONObject object, boolean stick) {
        Element element = new Element();
        element.name = object.optString("name", "");
        element.x = object.optString("dynamicX", element.x);
        element.y = object.optString("dynamicY", element.y);
        element.width = object.optDouble("width", element.width);
        element.height = object.optDouble("height", element.height);
        element.opacity = object.optDouble("opacity", element.opacity);
        element.corner = object.optDouble("cornerRadius", element.corner);
        element.colour = object.optInt("bgColor", element.colour);
        element.stroke = object.optInt("strokeColor", element.stroke);
        element.strokeWidth = object.optDouble("strokeWidth", element.strokeWidth);
        element.toggle = object.optBoolean("isToggle", element.toggle);
        element.swipeable = object.optBoolean("isSwipeable", element.swipeable);
        element.inGame = object.optBoolean("displayInGame", element.inGame);
        element.inMenu = object.optBoolean("displayInMenu", element.inMenu);
        element.stick = stick;
        element.forwardLock = object.optBoolean("forwardLock", element.forwardLock);
        element.absolute = object.optBoolean("absolute", element.absolute);
        JSONArray keys = object.optJSONArray("keycodes");
        if (keys == null) {
            keys = object.optJSONArray("properties");
        }
        if (keys != null) {
            for (int i = 0; i < keys.length() && i < 4; i++) {
                element.keys[i] = keys.optInt(i);
            }
        }
        return element;
    }

    /** Write it back, with the same names, so another launcher can read it. */
    public static String write(Layout layout) throws Exception {
        JSONObject root = new JSONObject();
        JSONArray buttons = new JSONArray();
        JSONArray sticks = new JSONArray();
        JSONArray drawers = new JSONArray();
        for (Element element : layout.elements) {
            if (element.sub.isEmpty()) {
                (element.stick ? sticks : buttons).put(json(element));
            } else {
                JSONObject drawer = json(element);
                drawer.put("orientation", element.direction);
                JSONArray sub = new JSONArray();
                for (Element child : element.sub) {
                    sub.put(json(child));
                }
                drawer.put("buttonProperties", sub);
                drawers.put(drawer);
            }
        }
        try {
            root.put("version", layout.version);
            root.put("scaledAt", layout.scaledAt);
            root.put("mControlDataList", buttons);
            root.put("mJoystickDataList", sticks);
            root.put("mDrawerDataList", drawers);
        } catch (Exception impossible) {
            throw new IllegalStateException(impossible);
        }
        return root.toString(2);
    }

    private static JSONObject json(Element element) {
        JSONObject object = new JSONObject();
        keys(element, object);
        return object;
    }

    private static void keys(Element element, JSONObject object) {
        try {
            object.put("name", element.name);
            object.put("dynamicX", element.x);
            object.put("dynamicY", element.y);
            object.put("width", element.width);
            object.put("height", element.height);
            object.put("opacity", element.opacity);
            object.put("cornerRadius", element.corner);
            object.put("bgColor", element.colour);
            object.put("strokeColor", element.stroke);
            object.put("strokeWidth", element.strokeWidth);
            object.put("isToggle", element.toggle);
            object.put("isSwipeable", element.swipeable);
            object.put("displayInGame", element.inGame);
            object.put("displayInMenu", element.inMenu);
            if (element.stick) {
                object.put("forwardLock", element.forwardLock);
                object.put("absolute", element.absolute);
            }
            JSONArray keys = new JSONArray();
            for (int key : element.keys) {
                keys.put(key);
            }
            object.put(element.stick ? "properties" : "keycodes", keys);
        } catch (Exception impossible) {
            throw new IllegalStateException(impossible);
        }
    }

    /**
     * The layout a player gets before they make one, in the arrangement the
     * Android launchers that came before this one settled on: a cross of square
     * walking plates under the left thumb, the screens you reach for in wide
     * plates at the top-left, jump and inventory under the right thumb, and no
     * dig or place button at all — those are the glass, held still to break and
     * tapped to place.
     *
     * The plates are the same shape and colour there: 50 dp square, 80 by 30 wide,
     * black at thirty per cent, sharp corners, no outline.
     */
    public static Layout defaults() {
        Layout layout = new Layout();
        layout.elements.add(wide("F3", Keys.F3, "${margin}", "${margin}"));
        layout.elements.add(wide("T", Keys.T, "${margin} * 2 + ${width}", "${margin}"));
        layout.elements.add(wide("TAB", Keys.TAB, "${margin} * 4 + ${width} * 3", "${margin}"));
        layout.elements.add(wide("F5", Keys.F5, "${margin}", "${height} + ${margin}"));
        layout.elements.add(wide("ESC", Keys.ESCAPE, "${margin}", "${height} * 2 + ${margin} * 2"));
        layout.elements.add(square("W", Keys.W, "${margin} * 2 + ${width}",
                "${bottom} - ${margin} * 3 - ${height} * 2"));
        layout.elements.add(square("A", Keys.A, "${margin}",
                "${bottom} - ${margin} * 2 - ${height}"));
        layout.elements.add(square("S", Keys.S, "${margin} * 2 + ${width}",
                "${bottom} - ${margin}"));
        layout.elements.add(square("D", Keys.D, "${margin} * 3 + ${width} * 2",
                "${bottom} - ${margin} * 2 - ${height}"));
        layout.elements.add(square("E", Keys.E, "${margin} * 3 + ${width} * 2",
                "${bottom} - ${margin}"));
        layout.elements.add(square("SHIFT", Keys.SHIFT, "${margin} * 2 + ${width}",
                "${screen_height} - ${margin} * 2 - ${height} * 2"));
        layout.elements.add(square("JUMP", Keys.SPACE, "${right} - ${margin} * 2 - ${width}",
                "${bottom} - ${margin} * 2 - ${height}"));
        return layout;
    }

    /** A walking plate: 50 dp square, and slid across to change direction. */
    private static Element square(String name, int key, String x, String y) {
        Element element = plate(name, key, x, y);
        element.width = 50;
        element.height = 50;
        element.swipeable = true;
        return element;
    }

    /** A screen's plate: 80 by 30, the wide shape the top-left row is made of. */
    private static Element wide(String name, int key, String x, String y) {
        Element element = plate(name, key, x, y);
        element.width = 80;
        element.height = 30;
        return element;
    }

    private static Element plate(String name, int key, String x, String y) {
        Element element = new Element();
        element.name = name;
        element.keys[0] = key;
        element.x = x;
        element.y = y;
        return element;
    }

    /** The names the editor offers, and the key each one stands for. */
    public static final class Keys {
        public static final int W = 87, A = 65, S = 83, D = 68;
        public static final int SPACE = 32, SHIFT = 340, CONTROL = 341, ALT = 342;
        public static final int E = 69, Q = 81, T = 84, F = 70, ESCAPE = 256;
        public static final int TAB = 258, ENTER = 257, BACKSPACE = 259;
        public static final int UP = 265, DOWN = 264, LEFT_ARROW = 263, RIGHT_ARROW = 262;
        public static final int F3 = 292, F5 = 294;
        /**
         * The mouse and the typing board are named with negative numbers, which is
         * what a layout written elsewhere uses for them: {@code 0} means "no key",
         * so a button cannot be both nothing and the left mouse button.
         */
        public static final int LEFT = -3, RIGHT = -4, MIDDLE = -6, BOARD = -1;
        public static final int WHEEL_UP = -7, WHEEL_DOWN = -8, MENU = -9;

        /** What the player sees for a code, in the picker and on the button. */
        public static String name(int code) {
            switch (code) {
                case W: return "W";
                case A: return "A";
                case S: return "S";
                case D: return "D";
                case SPACE: return "Space";
                case SHIFT: return "Shift";
                case CONTROL: return "Ctrl";
                case ALT: return "Alt";
                case E: return "E";
                case Q: return "Q";
                case T: return "T";
                case F: return "F";
                case ESCAPE: return "Esc";
                case TAB: return "Tab";
                case ENTER: return "Enter";
                case BACKSPACE: return "Back";
                case UP: return "Up";
                case DOWN: return "Down";
                case LEFT_ARROW: return "Left";
                case RIGHT_ARROW: return "Right";
                case F3: return "F3";
                case F5: return "F5";
                case LEFT: return "Dig";
                case RIGHT: return "Place";
                case MIDDLE: return "Pick block";
                case BOARD: return "Keyboard";
                case WHEEL_UP: return "Wheel up";
                case WHEEL_DOWN: return "Wheel down";
                case MENU: return "Escape";
                default: return code == 0 ? "None" : "#" + code;
            }
        }

        /** The names on offer in the editor, in the order a player looks for them. */
        public static final String[] PICK = {
            "None", "W", "A", "S", "D", "Space", "Shift", "Ctrl", "Alt", "E", "Q", "T",
            "F", "Esc", "Tab", "Enter", "Back", "Up", "Down", "Left", "Right", "F3", "F5",
            "Dig", "Place", "Pick block", "Keyboard", "Wheel up", "Wheel down",
        };

        /** The code behind a name from {@link #PICK}. */
        public static int code(String label) {
            switch (label) {
                case "None": return 0;
                case "W": return W;
                case "A": return A;
                case "S": return S;
                case "D": return D;
                case "Space": return SPACE;
                case "Shift": return SHIFT;
                case "Ctrl": return CONTROL;
                case "Alt": return ALT;
                case "E": return E;
                case "Q": return Q;
                case "T": return T;
                case "F": return F;
                case "Esc": return ESCAPE;
                case "Tab": return TAB;
                case "Enter": return ENTER;
                case "Back": return BACKSPACE;
                case "Up": return UP;
                case "Down": return DOWN;
                case "Left": return LEFT_ARROW;
                case "Right": return RIGHT_ARROW;
                case "F3": return F3;
                case "F5": return F5;
                case "Dig": return LEFT;
                case "Place": return RIGHT;
                case "Pick block": return MIDDLE;
                case "Keyboard": return BOARD;
                case "Wheel up": return WHEEL_UP;
                case "Wheel down": return WHEEL_DOWN;
                default: return 0;
            }
        }
    }
}
