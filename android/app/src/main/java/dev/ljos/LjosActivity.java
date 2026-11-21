package dev.ljos;

import android.os.Build;
import android.os.Bundle;
import android.util.SparseBooleanArray;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.View;
import android.view.WindowManager;
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

    private OnBackPressedCallback back;

    /**
     * The combining accent a dead key left waiting for the next key, or 0.
     * Only while a field is being edited: otherwise the keys reach the
     * application, which composes them itself (ljos_host's DeadKey).
     */
    private int accent;

    /** Keys whose press this activity typed, so their release is its too. */
    private final SparseBooleanArray typed = new SparseBooleanArray();

    @Override
    protected void onCreate(Bundle savedInstanceState) {
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
