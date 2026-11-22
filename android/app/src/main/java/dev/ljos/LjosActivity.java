package dev.ljos;

import android.content.ClipboardManager;
import android.content.pm.ApplicationInfo;
import android.graphics.Matrix;
import android.graphics.RectF;
import android.os.Build;
import android.os.Bundle;
import android.system.ErrnoException;
import android.system.Os;
import android.util.SparseBooleanArray;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.View;
import android.view.WindowManager;
import android.view.inputmethod.CursorAnchorInfo;
import android.view.inputmethod.EditorBoundsInfo;
import android.view.inputmethod.InputMethodManager;
import androidx.activity.BackEventCompat;
import androidx.activity.OnBackPressedCallback;
import androidx.core.graphics.Insets;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import com.google.androidgamesdk.GameActivity;
import com.google.androidgamesdk.TextInputOf;
import com.google.androidgamesdk.gametextinput.InputConnection;

/**
 * The activity a Ljós application runs in. The application itself lives on
 * a thread of its own for the whole process; this activity only lends it a
 * screen, tells it what of the screen the system keeps (insets), and hands
 * it Back.
 */
public class LjosActivity extends GameActivity {
    /** Back, to the application: a key, a button or a completed swipe. */
    private static native void nativeBack();

    /** A back swipe started. */
    private static native void nativeBackStarted(float progress);

    /** The back swipe moved: {@code progress} from 0 to 1. */
    private static native void nativeBackProgressed(float progress);

    /** The back swipe was let go short. */
    private static native void nativeBackCancelled();

    /**
     * What of the window the system keeps, in pixels, each as left, top,
     * right, bottom: its bars, the display's cutout, the keyboard.
     */
    private static native void nativeInsets(
            int barsLeft, int barsTop, int barsRight, int barsBottom,
            int cutoutLeft, int cutoutTop, int cutoutRight, int cutoutBottom,
            int imeLeft, int imeTop, int imeRight, int imeBottom);

    /**
     * The system's clipboard holds something else. Since Android 10 only
     * the application with the keyboard's focus hears of it, which is when
     * a paste can follow.
     */
    private static native void nativeClipboardChanged();

    private OnBackPressedCallback back;

    private final ClipboardManager.OnPrimaryClipChangedListener clipChanged =
            LjosActivity::nativeClipboardChanged;

    /**
     * The combining accent a dead key left waiting for the next key, or 0.
     * Only while a field is being edited: otherwise the keys reach the
     * application, which composes them itself (ljos_host's DeadKey).
     */
    private int accent;

    /** Keys whose press this activity typed, so their release is its too. */
    private final SparseBooleanArray typed = new SparseBooleanArray();

    /** The intent's extra that asks for the inspector (`ljos android run --mcp`). */
    private static final String MCP_EXTRA = "dev.ljos.MCP";

    /**
     * Hands the application the inspector's capabilities when the intent
     * asks for them, as {@code LJOS_MCP}, with its own files directory for
     * the token file as {@code LJOS_MCP_DIR}. Before the native side
     * starts, which reads them once; and only in a debuggable package,
     * whose files {@code run-as} reads for {@code ljos android connect}.
     */
    private void serveInspector() {
        String caps = getIntent() == null ? null : getIntent().getStringExtra(MCP_EXTRA);
        boolean debuggable = (getApplicationInfo().flags & ApplicationInfo.FLAG_DEBUGGABLE) != 0;
        if (caps == null || !debuggable) {
            return;
        }
        try {
            Os.setenv("LJOS_MCP", caps, true);
            Os.setenv("LJOS_MCP_DIR", getFilesDir().getAbsolutePath(), true);
        } catch (ErrnoException e) {
            android.util.Log.w("ljos", "the inspector is not served", e);
        }
    }

    /**
     * Where the application last said the editor, its cursor and what it
     * composes are (cursorAnchorChanged), or null; on the UI thread.
     */
    private Anchor anchor;

    /** An input method asked to be told each time the text moves. */
    private boolean monitored;

    /** An input method asked to be told at once, before anything was known. */
    private boolean immediate;

    /** Where the text is, in the surface's pixels. */
    private static final class Anchor {
        float[] editor;
        float[] cursor;
        int selStart;
        int selEnd;
        int composeStart;
        String composing;
        float[] chars;
    }

    @Override
    protected InputEnabledSurfaceView createSurfaceView() {
        return TextInputOf.surface(this, this::cursorUpdatesRequested);
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        serveInspector();
        super.onCreate(savedInstanceState);
        // The application draws under the system's bars and the cutout and
        // keeps clear of them itself (SafeArea), as on any phone.
        WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            WindowManager.LayoutParams attrs = getWindow().getAttributes();
            attrs.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(attrs);
        }
        // With targetSdk 36 the system's Back (a gesture or the key) only
        // reaches the activity's back dispatcher, never onBackPressed nor
        // the native side as a key. While the callback is enabled the
        // application takes Back; while it is not, the system does Back
        // itself and plays its own animation (setBackTaken).
        back = new OnBackPressedCallback(true) {
            @Override
            public void handleOnBackStarted(BackEventCompat event) {
                nativeBackStarted(event.getProgress());
            }

            @Override
            public void handleOnBackProgressed(BackEventCompat event) {
                nativeBackProgressed(event.getProgress());
            }

            @Override
            public void handleOnBackCancelled() {
                nativeBackCancelled();
            }

            @Override
            public void handleOnBackPressed() {
                nativeBack();
            }
        };
        getOnBackPressedDispatcher().addCallback(this, back);
        ClipboardManager clipboard = getSystemService(ClipboardManager.class);
        if (clipboard != null) {
            clipboard.addPrimaryClipChangedListener(clipChanged);
        }
    }

    @Override
    protected void onDestroy() {
        ClipboardManager clipboard = getSystemService(ClipboardManager.class);
        if (clipboard != null) {
            clipboard.removePrimaryClipChangedListener(clipChanged);
        }
        super.onDestroy();
    }

    /**
     * Whether the application takes Back, from its thread. Before Android
     * 13 a root activity's Back with no callback finishes it, and there is
     * no system animation to give way to, so the application keeps Back.
     */
    public void setBackTaken(boolean taken) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            return;
        }
        runOnUiThread(() -> back.setEnabled(taken));
    }

    /**
     * The application is about to write what the input method holds. On
     * the native side's thread, and waits for no other.
     */
    public void textInputWriteComing() {
        TextInputOf.writeComing(mSurfaceView);
    }

    /**
     * Whether what the keyboard's connection told last is a write, told
     * back. A read on the native side's thread, which waits for no other.
     */
    public boolean textInputToldWrite() {
        return TextInputOf.toldWrite(mSurfaceView);
    }

    /** The write announced was stale, and is not made. */
    public void textInputWriteDropped() {
        TextInputOf.writeDropped(mSurfaceView);
    }

    /**
     * The application set what the input method holds; GameTextInput does
     * not tell the keyboard, so a cursor the user moved by touch, or text
     * the application changed, would be unknown to it until it restarts.
     * Positions in UTF-16 units; -1 for no composition. Text that is
     * another ({@code replaced}) is read anew by restarting the input: the
     * keyboard's own copy of the text around the cursor would be stale.
     */
    public void inputStateChanged(
            int selStart, int selEnd, int composeStart, int composeEnd, boolean replaced) {
        runOnUiThread(() -> {
            View view = getWindow().getDecorView().findFocus();
            InputMethodManager imm = getSystemService(InputMethodManager.class);
            if (view == null || imm == null) {
                return;
            }
            if (replaced) {
                imm.restartInput(view);
            } else {
                imm.updateSelection(view, selStart, selEnd, composeStart, composeEnd);
            }
        });
    }

    /**
     * An input method asked where the text is on screen, at once, each
     * time it moves, or both; 0 when its connection closed. On the UI
     * thread.
     */
    private boolean cursorUpdatesRequested(int mode) {
        monitored = (mode & android.view.inputmethod.InputConnection.CURSOR_UPDATE_MONITOR) != 0;
        immediate = (mode & android.view.inputmethod.InputConnection.CURSOR_UPDATE_IMMEDIATE) != 0;
        // The system notes an immediate request only once this returns, and
        // until then drops an anchor unchanged from the last; tell it after.
        if (immediate && anchor != null) {
            mSurfaceView.post(() -> {
                if (immediate && anchor != null) {
                    immediate = false;
                    tellAnchor(anchor);
                }
            });
        }
        return mode != 0;
    }

    /**
     * Where the editor with focus, its cursor and what it composes are,
     * from the application's thread; in the surface's pixels, each box as
     * left, top, right, bottom. Positions in UTF-16 units; {@code
     * composeStart} -1 for nothing composed; {@code chars} four floats a
     * composed unit. An input method that asked is told.
     */
    public void cursorAnchorChanged(
            float[] editor, float[] cursor, int selStart, int selEnd,
            int composeStart, String composing, float[] chars) {
        Anchor now = new Anchor();
        now.editor = editor;
        now.cursor = cursor;
        now.selStart = selStart;
        now.selEnd = selEnd;
        now.composeStart = composeStart;
        now.composing = composing;
        now.chars = chars;
        runOnUiThread(() -> {
            anchor = now;
            if (monitored || immediate) {
                immediate = false;
                tellAnchor(now);
            }
        });
    }

    /**
     * No editor has focus now, or another took it, from the application's
     * thread: nothing is told of the last one's text.
     */
    public void cursorAnchorCleared() {
        runOnUiThread(() -> {
            anchor = null;
        });
    }

    /** Tells the input method where the text is, on the UI thread. */
    private void tellAnchor(Anchor a) {
        View view = mSurfaceView;
        InputMethodManager imm = getSystemService(InputMethodManager.class);
        if (view == null || imm == null) {
            return;
        }
        // The surface's pixels are the view's: it fills the window.
        int[] at = new int[2];
        view.getLocationOnScreen(at);
        Matrix toScreen = new Matrix();
        toScreen.setTranslate(at[0], at[1]);
        RectF editor = new RectF(a.editor[0], a.editor[1], a.editor[2], a.editor[3]);
        // Seen only where the surface shows it as well.
        RectF seen = new RectF(editor);
        if (!seen.intersect(0, 0, view.getWidth(), view.getHeight())) {
            seen.setEmpty();
        }
        CursorAnchorInfo.Builder info = new CursorAnchorInfo.Builder()
                .setMatrix(toScreen)
                .setSelectionRange(a.selStart, a.selEnd);
        if (a.composeStart >= 0) {
            info.setComposingText(a.composeStart, a.composing);
            for (int i = 0; i + 3 < a.chars.length; i += 4) {
                info.addCharacterBounds(
                        a.composeStart + i / 4,
                        a.chars[i], a.chars[i + 1], a.chars[i + 2], a.chars[i + 3],
                        shown(seen, a.chars[i], a.chars[i + 1], a.chars[i + 2], a.chars[i + 3]));
            }
        }
        // The text's baseline is not known here: the cursor's bottom is the
        // nearest to it.
        float[] c = a.cursor;
        info.setInsertionMarkerLocation(
                c[0], c[1], c[3], c[3], shown(seen, c[0], c[1], c[2], c[3]));
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            info.setEditorBoundsInfo(new EditorBoundsInfo.Builder()
                    .setEditorBounds(editor)
                    .setHandwritingBounds(editor)
                    .build());
        }
        imm.updateCursorAnchorInfo(view, info.build());
    }

    /**
     * Whether a box is seen in what of the editor the surface shows, as
     * CursorAnchorInfo flags it; nothing is seen in an empty one.
     */
    private static int shown(RectF seen, float l, float t, float r, float b) {
        boolean in = !seen.isEmpty()
                && l >= seen.left && r <= seen.right && t >= seen.top && b <= seen.bottom;
        boolean out = seen.isEmpty()
                || r < seen.left || l > seen.right || b < seen.top || t > seen.bottom;
        int flags = 0;
        if (!out) {
            flags |= CursorAnchorInfo.FLAG_HAS_VISIBLE_REGION;
        }
        if (!in) {
            flags |= CursorAnchorInfo.FLAG_HAS_INVISIBLE_REGION;
        }
        return flags;
    }

    /**
     * While a field is being edited GameTextInput takes a hardware key's
     * press and types its character at once, so a dead key's accent comes
     * out on its own (´e, not é). Here the accent waits for the next key
     * and goes onto it, as ljos_host's DeadKey does when no field is being
     * edited: what the keyboard's map does not compose types the accent,
     * then the key; a key that types nothing drops the accent.
     */
    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        int code = event.getKeyCode();
        // A key this typed has its release swallowed even when editing
        // stopped while it was held: the release is the press's.
        if (event.getAction() == KeyEvent.ACTION_UP && typed.get(code)) {
            typed.delete(code);
            return true;
        }
        InputConnection editing = TextInputOf.connection(mSurfaceView);
        if (editing == null || !editing.getSoftKeyboardActive()) {
            accent = 0;
            return super.dispatchKeyEvent(event);
        }
        if (event.getAction() != KeyEvent.ACTION_DOWN) {
            return super.dispatchKeyEvent(event);
        }
        int c = event.getUnicodeChar(event.getMetaState());
        if (c == 0) {
            if (!KeyEvent.isModifierKey(code)) {
                accent = 0;
            }
            return super.dispatchKeyEvent(event);
        }
        boolean dead = (c & KeyCharacterMap.COMBINING_ACCENT) != 0;
        int pressed = c & KeyCharacterMap.COMBINING_ACCENT_MASK;
        String text = null;
        if (accent == 0) {
            if (!dead) {
                return super.dispatchKeyEvent(event);
            }
            accent = pressed;
        } else {
            int both = KeyEvent.getDeadChar(accent, pressed);
            if (both != 0) {
                text = codePoint(both);
                accent = 0;
            } else if (dead) {
                text = spacing(accent);
                accent = pressed;
            } else {
                text = spacing(accent) + codePoint(pressed);
                accent = 0;
            }
        }
        if (text != null) {
            editing.commitText(text, 1);
        }
        typed.put(code, true);
        return true;
    }

    /** The accent as a character of its own, as the accent typed before a space. */
    private static String spacing(int accent) {
        int alone = KeyEvent.getDeadChar(accent, ' ');
        return codePoint(alone != 0 ? alone : accent);
    }

    private static String codePoint(int c) {
        return new String(Character.toChars(c));
    }

    @Override
    public WindowInsetsCompat onApplyWindowInsets(View v, WindowInsetsCompat insets) {
        WindowInsetsCompat out = super.onApplyWindowInsets(v, insets);
        Insets bars = insets.getInsets(WindowInsetsCompat.Type.systemBars());
        Insets cutout = insets.getInsets(WindowInsetsCompat.Type.displayCutout());
        Insets ime = insets.getInsets(WindowInsetsCompat.Type.ime());
        nativeInsets(
                bars.left, bars.top, bars.right, bars.bottom,
                cutout.left, cutout.top, cutout.right, cutout.bottom,
                ime.left, ime.top, ime.right, ime.bottom);
        return out;
    }
}
