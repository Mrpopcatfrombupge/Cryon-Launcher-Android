package dev.cryon.client;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.LinearGradient;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.RadialGradient;
import android.graphics.Shader;
import android.util.AttributeSet;
import android.view.Choreographer;
import android.view.View;

/**
 * The moving shadow behind the interface.
 *
 * Three soft bodies of light drift slowly over black, on paths that do not repeat
 * for minutes, so the page is never a still image but is never busy either. This is
 * the identity of the screen: depth that moves like smoke, rather than decoration
 * sitting on top of a flat colour.
 *
 * It is deliberately cheap. The gradients are built once and only their matrices
 * change per frame; the clock is the system's own; the rate is capped at thirty
 * frames a second; and the clock stops the moment the view leaves the window,
 * because a launcher that costs battery while nobody is looking at it has failed at
 * the one job it has.
 */
public class ShadowBackdrop extends View implements Choreographer.FrameCallback {
    /** A wash over the whole page, or a glow along the sidebar's edge. */
    public enum Kind {
        PAGE,
        SIDEBAR
    }

    private static final long FRAME_PERIOD_NS = 1_000_000_000L / 20;

    private final Kind kind;
    private final Paint body = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint edge = new Paint();
    private final RadialGradient[] lights = new RadialGradient[3];
    private final Matrix[] moves = new Matrix[3];
    private final float[] spread = {0.85f, 1.15f, 0.65f};
    /** Phase and the two speeds per body, in seconds, none of them related. */
    private final float[][] drift = {
        {0.0f, 0.031f, 0.017f},
        {2.1f, 0.019f, 0.026f},
        {4.3f, 0.024f, 0.013f},
    };
    private long lastFrameNs;
    private final long epochNs = System.nanoTime();
    private boolean running;
    private boolean motion;

    public ShadowBackdrop(Context context) {
        this(context, Kind.PAGE);
    }

    /**
     * So a screen can put the backdrop in its layout rather than build it.
     *
     * The AttributeSet reaches super(): a view's id is read there, and a backdrop
     * that dropped it could never be found by findViewById.
     */
    public ShadowBackdrop(Context context, AttributeSet attrs) {
        super(context, attrs);
        this.kind = Kind.PAGE;
        finish();
    }

    public ShadowBackdrop(Context context, Kind kind) {
        super(context);
        this.kind = kind;
        finish();
    }

    /** The paints and gradients every constructor shares. */
    private void finish() {
        // Cold silver-blue and one near-white, all of them faint: on black, three
        // lights at a fifth of full already read as a room, and anything brighter
        // becomes a poster.
        int[] colours = {0x2E8FB4D4, 0x24C8CCD3, 0x1E4E7796};
        for (int i = 0; i < lights.length; i++) {
            lights[i] = new RadialGradient(0, 0, 1, colours[i], 0x00000000,
                    Shader.TileMode.CLAMP);
            moves[i] = new Matrix();
        }
        edge.setShader(new LinearGradient(0, 0, 0, 1, 0x00000000, 0x99000000,
                Shader.TileMode.CLAMP));
    }

    @Override
    protected void onAttachedToWindow() {
        super.onAttachedToWindow();
        refreshMotion();
    }

    /** Static by default. No callback survives a hidden or unfocused launcher. */
    public void refreshMotion() {
        motion = "1".equals(Cryon.open(getContext()).preference("launcherMotion", "0"))
                && android.animation.ValueAnimator.areAnimatorsEnabled();
        updateClock();
        invalidate();
    }

    private void updateClock() {
        boolean active = motion && isAttachedToWindow() && isShown()
                && getWindowVisibility() == VISIBLE && hasWindowFocus();
        Choreographer clock = Choreographer.getInstance();
        clock.removeFrameCallback(this);
        running = active;
        if (active) {
            lastFrameNs = 0;
            clock.postFrameCallback(this);
        }
    }

    @Override
    public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused);
        if (focused) {
            refreshMotion();
        } else {
            updateClock();
        }
    }

    @Override
    protected void onWindowVisibilityChanged(int visibility) {
        super.onWindowVisibilityChanged(visibility);
        updateClock();
    }

    @Override
    protected void onDetachedFromWindow() {
        running = false;
        Choreographer.getInstance().removeFrameCallback(this);
        super.onDetachedFromWindow();
    }

    @Override
    public void doFrame(long frameTimeNanos) {
        if (!running) {
            return;
        }
        if (frameTimeNanos - lastFrameNs >= FRAME_PERIOD_NS) {
            lastFrameNs = frameTimeNanos;
            invalidate();
        }
        Choreographer.getInstance().postFrameCallbackDelayed(this, 50);
    }

    @Override
    protected void onDraw(Canvas canvas) {
        float width = getWidth();
        float height = getHeight();
        if (width <= 0f || height <= 0f) {
            return;
        }
        canvas.drawColor(0xFF000000);
        float seconds = motion ? (System.nanoTime() - epochNs) / 1_000_000_000f : 12f;
        float span = Math.max(width, height);
        for (int i = 0; i < lights.length; i++) {
            float phase = seconds * drift[i][1] + drift[i][0];
            float x;
            float y;
            if (kind == Kind.SIDEBAR) {
                // A sidebar's shadow belongs to its edge, so the bodies are pulled
                // into a band along it instead of spread over the page.
                x = width * (0.18f + 0.3f * (float) Math.sin(phase));
                y = height * (0.5f + 0.46f * (float) Math.sin(phase * 0.83f + i * 1.9f));
            } else {
                // Two unrelated frequencies per body: no light is where it was a
                // minute ago, which is what stops the drift reading as a loop.
                x = width * (0.5f + 0.42f * (float) Math.sin(phase));
                y = height * (0.5f + 0.38f * (float) Math.sin(phase * 1.37f + i));
            }
            float radius = span * spread[i] * (kind == Kind.SIDEBAR ? 0.55f : 0.7f);
            moves[i].setScale(radius, radius);
            moves[i].postTranslate(x, y);
            lights[i].setLocalMatrix(moves[i]);
            body.setShader(lights[i]);
            canvas.drawRect(0, 0, width, height, body);
        }
        if (kind == Kind.SIDEBAR) {
            // A hard falloff at the bottom edge, so the panel lifts off the page
            // rather than stopping where its colour does. The shader is one unit
            // tall and is stretched here, which keeps it out of the frame budget.
            canvas.save();
            canvas.scale(1f, height);
            canvas.drawRect(0, 0, width, height, edge);
            canvas.restore();
        }
    }
}
