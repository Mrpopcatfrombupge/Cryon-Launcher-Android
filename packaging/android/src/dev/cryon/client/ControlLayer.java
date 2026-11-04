package dev.cryon.client;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.view.MotionEvent;
import android.view.View;
import android.widget.FrameLayout;

import java.util.ArrayList;
import java.util.List;

/**
 * The control layout, laid out and editable.
 *
 * This is the editor's canvas: every element of the layout is a view here, placed
 * by the same expression the game reads, so what a player arranges is what the
 * game answers with. A control is dragged with a finger and its corner pulls it
 * bigger; either way the element keeps an expression rather than a number, so the
 * same layout lands in the same corner on a phone of any size.
 *
 * The game draws its own copy of this over the world. It has to: a control that
 * lives in the launcher's window would be hidden by the game's surface, which the
 * platform orders above everything else in that window, and a separate window for
 * it can be seen but never touched.
 */
public final class ControlLayer extends FrameLayout {
    /** What the player's settings say about how the controls look. */
    public static final class Tuning {
        /** Button size, as a percentage: 100 is the size the layout was saved at. */
        public double scale = 100;
    }

    /** The editor's side of the view: which control the player touched. */
    public interface Listener {
        void picked(Layout.Element element);
    }

    private final Layout layout;
    private final Tuning tuning;
    private final float density;
    private final List<Face> faces = new ArrayList<>();
    private Listener listener;

    public ControlLayer(Context context, Layout layout, Tuning tuning) {
        super(context);
        this.layout = layout;
        this.tuning = tuning == null ? new Tuning() : tuning;
        this.density = context.getResources().getDisplayMetrics().density;
        build();
    }

    public void setListener(Listener listener) {
        this.listener = listener;
    }

    /** The layout on show, which the editor changes in place. */
    public Layout layout() {
        return layout;
    }

    /** Start again from the layout: the editor calls this after any change. */
    public void rebuild() {
        build();
    }

    @Override
    protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
        place(width, height);
    }

    /** Put every element where its expression says, for this screen. */
    private void place(int width, int height) {
        for (Face face : faces) {
            Layout.Element element = face.element;
            double scale = tuning.scale / Math.max(1, layout.scaledAt);
            int w = (int) Math.round(element.width * density * scale);
            int h = (int) Math.round(element.height * density * scale);
            int x = (int) Math.round(Anchor.evaluate(element.x, width, height,
                    density, w, h));
            int y = (int) Math.round(Anchor.evaluate(element.y, width, height,
                    density, w, h));
            FrameLayout.LayoutParams params =
                    (FrameLayout.LayoutParams) face.getLayoutParams();
            params.leftMargin = x;
            params.topMargin = y;
            params.width = w;
            params.height = h;
            face.setLayoutParams(params);
            face.invalidate();
        }
    }

    /** Rebuild the views from the layout. */
    private void build() {
        removeAllViews();
        faces.clear();
        for (Layout.Element element : layout.elements) {
            Face face = new Face(getContext(), element);
            faces.add(face);
            addView(face, new FrameLayout.LayoutParams(1, 1));
        }
        if (getWidth() > 0) {
            place(getWidth(), getHeight());
        }
    }

    /** One control, drawn as its layout describes it. */
    private class Face extends View {
        final Layout.Element element;
        private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint edge = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint words = new Paint(Paint.ANTI_ALIAS_FLAG);
        private int lastX, lastY;
        private boolean resizing;

        Face(Context context, Layout.Element element) {
            super(context);
            this.element = element;
            words.setTextAlign(Paint.Align.CENTER);
            words.setColor(0xFFFFFFFF);
        }

        @Override
        protected void onDraw(Canvas canvas) {
            float width = getWidth(), height = getHeight();
            float radius = Math.min(width, height) / 2
                    * (float) Math.min(100, Math.max(0, element.corner)) / 100f;
            RectF box = new RectF(0, 0, width, height);
            fill.setStyle(Paint.Style.FILL);
            fill.setColor(tint(element.colour, element.opacity));
            canvas.drawRoundRect(box, radius, radius, fill);
            if (element.strokeWidth > 0) {
                edge.setStyle(Paint.Style.STROKE);
                edge.setStrokeWidth((float) (element.strokeWidth * density));
                edge.setColor(tint(element.stroke, element.opacity));
                canvas.drawRoundRect(box, radius, radius, edge);
            }
            if (!element.name.isEmpty() && !mark(canvas, width, height)) {
                words.setTextSize(Math.min(height * 0.4f, 15 * density));
                canvas.drawText(element.name.toUpperCase(), width / 2,
                        height / 2 - (words.descent() + words.ascent()) / 2, words);
            }
        }

        /**
         * The mouse or keyboard mark, so the plate in the editor is the plate in
         * the game: the same hand, the same shape, and no word to translate.
         */
        private boolean mark(Canvas canvas, float width, float height) {
            int code = element.keys.length > 0 ? element.keys[0] : 0;
            boolean left = code == Layout.Keys.LEFT, right = code == Layout.Keys.RIGHT;
            boolean middle = code == Layout.Keys.MIDDLE, board = code == Layout.Keys.BOARD;
            if (!(left || right || middle || board)) {
                return false;
            }
            Paint ink = new Paint(Paint.ANTI_ALIAS_FLAG);
            ink.setColor(tint(element.stroke, element.opacity));
            ink.setStyle(Paint.Style.STROKE);
            ink.setStrokeWidth(2 * density);
            Paint lit = new Paint(Paint.ANTI_ALIAS_FLAG);
            lit.setColor(tint(0xFFFFFFFF, Math.min(1, element.opacity + 0.3)));
            float radius = 12 * density;
            if (board) {
                float bw = width * 0.58f, bh = height * 0.38f;
                float bx = (width - bw) / 2, by = (height - bh) / 2;
                canvas.drawRoundRect(new RectF(bx, by, bx + bw, by + bh), bh * 0.22f, bh * 0.22f, ink);
                for (int row = 0; row < 2; row++) {
                    for (int column = 0; column < 4; column++) {
                        float kx = bx + bw * 0.12f + column * bw * 0.2f;
                        float ky = by + bh * 0.22f + row * bh * 0.42f;
                        canvas.drawRect(kx, ky, kx + bw * 0.13f, ky + bh * 0.26f, lit);
                    }
                }
                return true;
            }
            float mw = width * 0.36f, mh = height * 0.54f;
            float mx = (width - mw) / 2, my = (height - mh) / 2;
            canvas.drawRoundRect(new RectF(mx, my, mx + mw, my + mh), radius, radius, ink);
            float half = mw / 2;
            if (left || middle) {
                canvas.drawRect(mx, my, mx + half, my + mh * 0.42f, lit);
            }
            if (right || middle) {
                canvas.drawRect(mx + half, my, mx + mw, my + mh * 0.42f, lit);
            }
            if (middle) {
                canvas.drawRect(mx + half * 0.55f, my + mh * 0.08f,
                        mx + half * 1.45f, my + mh * 0.26f, lit);
            }
            return true;
        }

        @Override
        public boolean onTouchEvent(MotionEvent event) {
            int action = event.getActionMasked();
            if (action == MotionEvent.ACTION_DOWN) {
                if (listener != null) {
                    listener.picked(element);
                }
                lastX = Math.round(event.getRawX());
                lastY = Math.round(event.getRawY());
                resizing = event.getX() > getWidth() - 30 * density
                        && event.getY() > getHeight() - 30 * density;
                return true;
            }
            if (action == MotionEvent.ACTION_MOVE) {
                move(Math.round(event.getRawX()) - lastX,
                        Math.round(event.getRawY()) - lastY);
                lastX = Math.round(event.getRawX());
                lastY = Math.round(event.getRawY());
                return true;
            }
            if (action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_CANCEL) {
                resizing = false;
                return true;
            }
            return true;
        }

        /** Drag a control about, or pull its corner to make it bigger. */
        private void move(int dx, int dy) {
            FrameLayout.LayoutParams params =
                    (FrameLayout.LayoutParams) getLayoutParams();
            if (resizing) {
                params.width = Math.max(24, params.width + dx);
                params.height = Math.max(24, params.height + dy);
                element.width = params.width / density;
                element.height = params.height / density;
            } else {
                params.leftMargin += dx;
                params.topMargin += dy;
                anchor(params.leftMargin, params.topMargin, params.width, params.height);
            }
            setLayoutParams(params);
            invalidate();
        }

        /**
         * Keep the position as an expression, measured from whichever edge it is
         * nearer. A layout that says `px(1800)` is wrong on any other phone; one
         * that says `${right} - ${width} - px(24)` is the same corner everywhere.
         */
        private void anchor(int x, int y, int width, int height) {
            int screen = getWidth() == 0 ? getResources().getDisplayMetrics().widthPixels
                    : ControlLayer.this.getWidth();
            int tall = ControlLayer.this.getHeight() == 0
                    ? getResources().getDisplayMetrics().heightPixels
                    : ControlLayer.this.getHeight();
            element.x = x + width / 2 < screen / 2
                    ? "${left} + px(" + x + ")"
                    : "${right} - px(" + (screen - x - width) + ") - ${width}";
            element.y = y + height / 2 < tall / 2
                    ? "${top} + px(" + y + ")"
                    : "${bottom} - px(" + (tall - y - height) + ") - ${height}";
        }
    }

    /** The layout's colour, with the element's own opacity. */
    private static int tint(int rgb, double opacity) {
        int alpha = (int) Math.round(Math.max(0, Math.min(1, opacity)) * 255);
        return (alpha << 24) | (rgb & 0x00FFFFFF);
    }
}
