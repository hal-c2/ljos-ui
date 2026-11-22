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
    private final GameActivity.InputEnabledSurfaceView surface;
    private final InputConnection connection;
    private final Handler main = new Handler(Looper.getMainLooper());

    private Cases(boolean shown, boolean served) {
        Handler.runQueued();
        InputMethodManager.served = served;
        surface = TextInputOf.surface(activity, mode -> true);
        view = surface;
        connection = TextInputOf.connection(surface);
        if (shown) {
            connection.setSoftKeyboardActive(true, 0);
        }
        Handler.runQueued();
    }

    /** The native side's write, landed as it is made. */
    private void write(String text, int cursor, int composeStart, int composeEnd) {
        TextInputOf.writeComing(surface);
        land(text, cursor, composeStart, composeEnd);
    }

    /** A write issued earlier reaches the UI thread; the newest state is what lands. */
    private void land(String text, int cursor, int composeStart, int composeEnd) {
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

    private static void writeToldBackSaysSo() {
        Cases it = new Cases(true, true).hello();
        TextInputOf.writeComing(it.surface);
        it.connection.commitText("!", 1);
        same("a report says it is a write", TextInputOf.toldWrite(it.surface) ? 1 : 0, 0);
        it.land("w1", 2, -1, -1);
        TextInputOf.writeComing(it.surface);
        Handler.runQueued();
        it.told("a write told back with the next on its way", "hello!|6,6|-1,-1", "w1|2,2|-1,-1");
        same("a write told back says so", TextInputOf.toldWrite(it.surface) ? 1 : 0, 1);
        it.land("w3", 2, -1, -1);
        Handler.runQueued();
        it.connection.commitText("?", 1);
        same("typing after it says it is a write", TextInputOf.toldWrite(it.surface) ? 1 : 0, 0);
    }

    private static void overtakenReportIsFollowedByTheWrite() {
        for (boolean served : BOTH) {
            for (boolean composing : BOTH) {
                String what = "a write over a report, served=" + served + " composing=" + composing;
                Cases it = new Cases(true, served).hello();
                TextInputOf.writeComing(it.surface);
                it.connection.commitText("!", 1);
                it.told(what + ", the report is not held", "hello!|6,6|-1,-1");
                it.land("hi", 2, composing ? 0 : -1, composing ? 2 : -1);
                InputMethodManager.restartInput(it.connection);
                Handler.runQueued();
                InputMethodManager.restartInput(it.connection);
                Handler.runQueued();
                it.told(what, "hello!|6,6|-1,-1", composing ? "hi|2,2|0,2" : "hi|2,2|-1,-1");
            }
        }

        Cases moved = new Cases(true, true).hello();
        TextInputOf.writeComing(moved.surface);
        moved.connection.setSelection(1, 1);
        Handler.runQueued();
        moved.land("hello", 5, -1, -1);
        Handler.runQueued();
        moved.told("a write over a move", "hello|1,1|-1,-1", "hello|5,5|-1,-1");

        Cases keyed = new Cases(true, true).hello();
        TextInputOf.writeComing(keyed.surface);
        keyed.connection.onKey(keyed.view, 21, new KeyEvent());
        Handler.runQueued();
        keyed.land("hello", 5, -1, -1);
        Handler.runQueued();
        keyed.told("a write over a key", "hello|4,4|-1,-1", "hello|5,5|-1,-1");
    }

    private static void writesOnTheirWayAreToldOnce() {
        Cases it = new Cases(true, true).hello();
        TextInputOf.writeComing(it.surface);
        TextInputOf.writeComing(it.surface);
        it.connection.commitText("!", 1);
        it.land("ab", 2, -1, -1);
        it.land("ab", 2, -1, -1);
        Handler.runQueued();
        it.told("two writes over a report", "hello!|6,6|-1,-1", "ab|2,2|-1,-1");

        // The first to land doesn't end the second's race.
        it = new Cases(true, true).hello();
        TextInputOf.writeComing(it.surface);
        TextInputOf.writeComing(it.surface);
        it.land("ab", 2, -1, -1);
        Handler.runQueued();
        it.connection.commitText("c", 1);
        it.land("ab", 2, -1, -1);
        Handler.runQueued();
        it.told("a report between two writes", "abc|3,3|-1,-1", "ab|2,2|-1,-1");
    }

    private static void writeAcrossTheToldWriteIsToldInItsTurn() {
        Cases it = new Cases(true, true).hello();
        TextInputOf.writeComing(it.surface);
        it.connection.commitText("!", 1);
        it.land("hi", 2, -1, -1);
        // Announced before what landed was told.
        TextInputOf.writeComing(it.surface);
        Handler.runQueued();
        it.land("ho", 2, -1, -1);
        Handler.runQueued();
        it.told("a write across the told write",
                "hello!|6,6|-1,-1", "hi|2,2|-1,-1", "ho|2,2|-1,-1");
    }

    private static void writeThatNeverLandsHoldsNothing() {
        Cases lost = new Cases(true, true).hello();
        TextInputOf.writeComing(lost.surface);
        lost.connection.commitText("!", 1);
        lost.connection.setSelection(1, 1);
        Handler.runQueued();
        lost.told("a write that was lost", "hello!|6,6|-1,-1", "hello!|1,1|-1,-1");

        Cases dropped = new Cases(true, true).hello();
        TextInputOf.writeComing(dropped.surface);
        TextInputOf.writeDropped(dropped.surface);
        dropped.connection.commitText("!", 1);
        dropped.write("hi", 2, -1, -1);
        Handler.runQueued();
        dropped.told("a write the native side dropped", "hello!|6,6|-1,-1");
    }

    public static void main(String[] args) {
        writtenStateIsNotToldBack();
        secondWriteStays();
        movedCursorIsToldOnce();
        moveIsToldBeforeTheNextEdit();
        moveOutlivesShowingAndHiding();
        endedCompositionIsTold();
        overtakenReportIsFollowedByTheWrite();
        writeToldBackSaysSo();
        writesOnTheirWayAreToldOnce();
        writeAcrossTheToldWriteIsToldInItsTurn();
        writeThatNeverLandsHoldsNothing();
        FAILED.forEach(System.out::println);
        System.out.println(FAILED.isEmpty() ? "every case holds" : FAILED.size() + " failed");
        System.exit(FAILED.isEmpty() ? 0 : 1);
    }
}
