package com.google.androidgamesdk;

import android.os.Handler;
import android.os.Looper;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.InputMethodManager;
import com.google.androidgamesdk.gametextinput.InputConnection;
import com.google.androidgamesdk.gametextinput.State;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/** What the real TextInputOf's connection tells the native side, and when. */
public final class Cases {
    private static final List<String> FAILED = new ArrayList<>();
    private static final boolean[] BOTH = {false, true};

    private final GameActivity activity = new GameActivity();
    private final View view;
    private final InputConnection connection;
    private final Handler main = new Handler(Looper.getMainLooper());

    private Cases(boolean shown, boolean served) {
        Handler.runQueued();
        InputMethodManager.served = served;
        GameActivity.InputEnabledSurfaceView surface = TextInputOf.surface(activity, mode -> true);
        view = surface;
        connection = TextInputOf.connection(surface);
        if (shown) {
            connection.setSoftKeyboardActive(true, 0);
        }
        Handler.runQueued();
    }

    private void write(String text, int cursor, int composeStart, int composeEnd) {
        connection.setState(new State(text, cursor, cursor, composeStart, composeEnd));
    }

    /** A field holding "hello", its cursor at the end, and nothing told yet. */
    private Cases hello() {
        write("hello", 5, -1, -1);
        Handler.runQueued();
        activity.told.clear();
        return this;
    }

    private void told(String what, String... states) {
        if (!activity.told.equals(Arrays.asList(states))) {
            FAILED.add(what + ": told " + activity.told + ", not " + Arrays.toString(states));
        }
    }

    private static void same(String what, int got, int wanted) {
        if (got != wanted) {
            FAILED.add(what + ": " + got + ", not " + wanted);
        }
    }

    private static void writtenStateIsNotToldBack() {
        for (boolean shown : BOTH) {
            for (boolean served : BOTH) {
                for (boolean composing : BOTH) {
                    String what = "a write, shown=" + shown + " served=" + served
                            + " composing=" + composing;
                    Cases it = new Cases(shown, served);
                    it.write("hello", 5, composing ? 1 : -1, composing ? 4 : -1);
                    // LjosActivity.inputStateChanged, for a replaced text.
                    InputMethodManager.restartInput(it.connection);
                    Handler.runQueued();
                    it.told(what);
                    same(what + ", composition kept",
                            InputConnection.getComposingSpanStart(it.connection.getEditable()),
                            composing ? 1 : -1);
                    same(what + ", cursor the keyboard is started with",
                            it.connection.getEditorInfo().initialSelStart, 5);
                }
            }
        }
    }

    private static void secondWriteStays() {
        Cases it = new Cases(true, true);
        // Any report here would forget "ab", on its way, and drop the next
        // write over it.
        it.write("a", 1, -1, -1);
        InputMethodManager.restartInput(it.connection);
        it.connection.finishComposingText();
        Handler.runQueued();
        it.told("the first of two writes");
        it.connection.setSoftKeyboardActive(false, 0);
        it.write("ab", 2, -1, -1);
        Handler.runQueued();
        it.told("the second of two writes");
    }

    private static void movedCursorIsToldOnce() {
        Cases it = new Cases(true, true).hello();
        it.connection.setSelection(1, 1);
        Handler.runQueued();
        InputMethodManager.restartInput(it.connection);
        it.told("a moved cursor", "hello|1,1|-1,-1");

        it = new Cases(true, true).hello();
        it.connection.setComposingRegion(1, 4);
        Handler.runQueued();
        it.told("a region to compose", "hello|5,5|1,4");
    }

    private static void moveIsToldBeforeTheNextEdit() {
        Cases typed = new Cases(true, true).hello();
        // Queued already when the move is made.
        typed.main.post(() -> typed.connection.commitText("X", 1));
        typed.connection.setSelection(1, 1);
        Handler.runQueued();
        typed.told("a move, then a commit", "hello|1,1|-1,-1", "hXello|2,2|-1,-1");

        Cases keyed = new Cases(true, true).hello();
        keyed.main.post(() -> keyed.connection.onKey(keyed.view, 21, new KeyEvent()));
        keyed.connection.setSelection(2, 2);
        Handler.runQueued();
        keyed.told("a move, then a key", "hello|2,2|-1,-1", "hello|1,1|-1,-1");
    }

    private static void moveOutlivesShowingAndHiding() {
        for (boolean show : BOTH) {
            for (boolean served : BOTH) {
                Cases it = new Cases(!show, served).hello();
                it.connection.setSelection(1, 1);
                // The native side's command, ahead of the queued report.
                it.connection.setSoftKeyboardActive(show, 0);
                Handler.runQueued();
                it.told("a move, then show=" + show + " served=" + served, "hello|1,1|-1,-1");
            }
        }
    }

    private static void endedCompositionIsTold() {
        Cases it = new Cases(true, true).hello();
        it.connection.setComposingText("!", 1);
        it.connection.setSoftKeyboardActive(false, 0);
        Handler.runQueued();
        it.told("a composition, then hide", "hello!|6,6|5,6", "hello!|6,6|-1,-1");
    }

    public static void main(String[] args) {
        writtenStateIsNotToldBack();
        secondWriteStays();
        movedCursorIsToldOnce();
        moveIsToldBeforeTheNextEdit();
        moveOutlivesShowingAndHiding();
        endedCompositionIsTold();
        FAILED.forEach(System.out::println);
        System.out.println(FAILED.isEmpty() ? "every case holds" : FAILED.size() + " failed");
        System.exit(FAILED.isEmpty() ? 0 : 1);
    }
}
