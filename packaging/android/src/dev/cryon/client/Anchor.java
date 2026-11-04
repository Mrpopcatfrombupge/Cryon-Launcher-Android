package dev.cryon.client;

import java.util.HashMap;
import java.util.Map;

/**
 * The little arithmetic a control layout is written in.
 *
 * An element says where it stands with an expression — `${right} - ${margin} * 2 -
 * ${width}` — rather than a number, because the same layout has to land in the
 * corner of every phone. The names are the ones a layout file already uses, so a
 * file written elsewhere is read here without being rewritten.
 *
 * `dp()` and `px()` convert: a bare number is dp, which is what a thumb needs, and
 * a layout that wants pixels says so.
 */
public final class Anchor {
    private final String text;
    private int at;
    private final Map<String, Double> variables;

    private Anchor(String text, Map<String, Double> variables) {
        this.text = text;
        this.variables = variables;
    }

    /**
     * Evaluate one expression for one screen.
     *
     * @param expression what the layout wrote
     * @param width      the screen's width in pixels
     * @param height     the screen's height in pixels
     * @param dp         the device's density, so dp becomes pixels
     * @param own        the element's own width, which its expression may name
     * @param ownHeight  the element's own height
     */
    public static double evaluate(String expression, double width, double height,
                                  double dp, double own, double ownHeight) {
        Map<String, Double> names = new HashMap<>();
        names.put("margin", 2 * dp);
        names.put("top", 0.0);
        names.put("left", 0.0);
        // `${right}` and `${bottom}` are the furthest an element's own corner may
        // sit, not the screen's edge: `${bottom} - ${margin}` is "one margin above
        // the bottom" with the plate whole, which is what every layout written
        // elsewhere means by it.
        names.put("right", width - own);
        names.put("bottom", height - ownHeight);
        names.put("screen_width", width);
        names.put("screen_height", height);
        names.put("width", own);
        names.put("height", ownHeight);
        names.put("preferred_scale", dp);
        try {
            return new Anchor(expression, names).sum();
        } catch (Exception malformed) {
            // A layout edited by hand can be wrong, and a button in the wrong
            // place is better than no button at all.
            return 0;
        }
    }

    /** The sum, which is the whole expression: terms joined by `+` and `-`. */
    private double sum() {
        double value = term();
        for (;;) {
            skip();
            if (accept('+')) {
                value += term();
            } else if (accept('-')) {
                value -= term();
            } else {
                return value;
            }
        }
    }

    private double term() {
        double value = factor();
        for (;;) {
            skip();
            if (accept('*')) {
                value *= factor();
            } else if (accept('/')) {
                value /= factor();
            } else if (accept('%')) {
                value %= factor();
            } else {
                return value;
            }
        }
    }

    private double factor() {
        skip();
        if (accept('-')) {
            return -factor();
        }
        if (accept('(')) {
            double value = sum();
            skip();
            accept(')');
            return value;
        }
        if (accept('$')) {
            return variable();
        }
        if (match("dp(")) {
            return number() * close();
        }
        if (match("px(")) {
            skip();
            double pixels = number();
            close();
            return pixels;
        }
        return number();
    }

    private double close() {
        double value = number();
        skip();
        accept(')');
        return value;
    }

    /** `${name}`, with the braces Pojav's files always write. */
    private double variable() {
        skip();
        if (!accept('{')) {
            return 0;
        }
        int start = at;
        while (at < text.length() && text.charAt(at) != '}') {
            at++;
        }
        String name = text.substring(start, at);
        accept('}');
        Double value = variables.get(name.trim());
        return value == null ? 0 : value;
    }

    private double number() {
        skip();
        int start = at;
        while (at < text.length() && (Character.isDigit(text.charAt(at))
                || text.charAt(at) == '.')) {
            at++;
        }
        if (at == start) {
            return 0;
        }
        return Double.parseDouble(text.substring(start, at));
    }

    private void skip() {
        while (at < text.length() && Character.isWhitespace(text.charAt(at))) {
            at++;
        }
    }

    private boolean accept(char expected) {
        skip();
        if (at < text.length() && text.charAt(at) == expected) {
            at++;
            return true;
        }
        return false;
    }

    /** Consume a whole word, if that is what comes next. */
    private boolean match(String word) {
        skip();
        if (text.regionMatches(at, word, 0, word.length())) {
            at += word.length();
            return true;
        }
        return false;
    }
}
