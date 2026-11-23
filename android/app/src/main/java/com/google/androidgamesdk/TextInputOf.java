package com.google.androidgamesdk;

import android.content.Context;
import android.os.Handler;
import android.os.Looper;
import android.text.Editable;
import android.text.Selection;
import android.text.Spanned;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.EditorInfo;
import com.google.androidgamesdk.gametextinput.InputConnection;
import com.google.androidgamesdk.gametextinput.Settings;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

/**
 * The connection GameTextInput keeps for the surface's editor. GameActivity
 * keeps it to its own package; LjosActivity needs it to type what a dead key
 * composes into the field being edited, the way an input method would, and
 * to hear an input method ask where the text is on screen, which
 * GameTextInput's own connection refuses, and to pass on a selection or
 * composing region an input method sets without changing the text, which
 * GameTextInput's own connection keeps to its Editable. What the native
 * side wrote itself is not told back to it, unless it landed on a state
 * told after it was written: the native side would keep that one.
 */
public final class TextInputOf {
    private TextInputOf() {}

    /** Told when an input method asks where the text is on screen. */
    public interface CursorUpdates {
        /**
         * The method asked for {@code mode}, the {@code CURSOR_UPDATE_}
         * flags of {@link android.view.inputmethod.InputConnection}; 0 once
         * its connection closed. On the UI thread. Whether it will be told.
         */
        boolean cursorUpdatesRequested(int mode);
    }

    /** The surface's connection, or null before an input method asked for one. */
    public static InputConnection connection(GameActivity.InputEnabledSurfaceView view) {
        return view == null ? null : view.mInputConnection;
    }

    /**
     * The native side is about to write: before it reads what was told
     * last, so whatever is told from here on is known to be overtaken. On
     * any thread; waits for none.
     */
    public static void writeComing(GameActivity.InputEnabledSurfaceView view) {
        if (connection(view) instanceof Connection) {
            ((Connection) connection(view)).writing.incrementAndGet();
        }
    }

    /** The write {@link #writeComing} announced was not made. */
    public static void writeDropped(GameActivity.InputEnabledSurfaceView view) {
        if (connection(view) instanceof Connection) {
            ((Connection) connection(view)).writing.decrementAndGet();
        }
    }

    /**
     * The listener is being told {@code state}, and has yet to hand it to
     * the native side, which asks {@link #toldWrite} once it has it. A write
     * told back is kept by what it holds: the native side takes the newest
     * state told, and a report told after it took one must not change what
     * it is told of that one.
     */
    public static void telling(
            GameActivity.InputEnabledSurfaceView view,
            com.google.androidgamesdk.gametextinput.State state) {
        if (connection(view) instanceof Connection) {
            Connection told = (Connection) connection(view);
            if (told.echoing) {
                told.toldWrite.set(
                        key(state.text, state.selectionStart, state.selectionEnd,
                                state.composingRegionStart, state.composingRegionEnd));
            }
        }
    }

    /**
     * Whether the state the native side took, as it took it, is a write
     * told back for having landed on one the input method made; the answer
     * is given once. On any thread; waits for none.
     */
    public static boolean toldWrite(
            GameActivity.InputEnabledSurfaceView view,
            String text, int selectionStart, int selectionEnd,
            int composingStart, int composingEnd) {
        if (!(connection(view) instanceof Connection)) {
            return false;
        }
        AtomicReference<String> told = ((Connection) connection(view)).toldWrite;
        String took = key(text, selectionStart, selectionEnd, composingStart, composingEnd);
        String echoed = told.get();
        // By identity: an echo told since, the same or not, stays to be asked.
        return took.equals(echoed) && told.compareAndSet(echoed, null);
    }

    /** A state as both sides read it: a region with an end below 0 is none. */
    private static String key(String text, int start, int end, int composeStart, int composeEnd) {
        boolean none = composeStart < 0 || composeEnd < 0;
        return start + "," + end + "," + (none ? -1 : composeStart) + ","
                + (none ? -1 : composeEnd) + "," + text;
    }

    /**
     * GameActivity's surface, its connection one that hands an input
     * method's {@code requestCursorUpdates} to {@code updates}.
     */
    public static GameActivity.InputEnabledSurfaceView surface(
            GameActivity activity, CursorUpdates updates) {
        GameActivity.InputEnabledSurfaceView view = activity.new InputEnabledSurfaceView(activity);
        EditorInfo info = view.mInputConnection.getEditorInfo();
        // As GameActivity sets up its own: key events are forwarded for an
        // editor of no type.
        view.mInputConnection =
                new Connection(activity, view, new Settings(info, info.inputType == 0), updates)
                        .setListener(activity);
        return view;
    }

    private static final class Connection extends InputConnection {
        private final CursorUpdates updates;
        private final Handler main = new Handler(Looper.getMainLooper());

        /**
         * The selection or composing region moved and the listener is yet
         * to be told. The UI thread's alone: an input method and
         * GameTextInput's setState both call in on it.
         */
        private boolean moved;

        /** Inside BaseInputConnection's batch that sets a composing region. */
        private boolean composing;

        /**
         * On the Editable from here on: gone when GameTextInput's setState,
         * which is final and clears every span, wrote a state.
         */
        private final Object unwritten = new Object();

        /** The last state told or written, as {@link #state} gives it. */
        private String known;

        /** Writes the native side announced and that have not landed. */
        final AtomicInteger writing = new AtomicInteger();

        /** A state was told with a write on its way, which will replace it. */
        private boolean overtaken;

        /**
         * The last write told back, as {@link #key} gives it, until the
         * native side asked about it; read on the native side's thread.
         */
        final AtomicReference<String> toldWrite = new AtomicReference<>();

        /** Inside the tell of a write back. */
        private boolean echoing;

        /** A write replaced a state told after it, and is yet to be told. */
        private boolean overwrote;

        /** The last of those was written. */
        private boolean holdsWrite;

        /** Inside GameTextInput's handling of a key, which tells by itself. */
        private boolean keyed;

        /** Inside closeConnection, which ends a composition. */
        private boolean closing;

        Connection(Context context, View view, Settings settings, CursorUpdates updates) {
            super(context, view, settings);
            this.updates = updates;
            mark();
            known = state();
        }

        private void mark() {
            getEditable().setSpan(unwritten, 0, 0, Spanned.SPAN_INCLUSIVE_INCLUSIVE);
        }

        /** All that the listener is told of the Editable, to compare. */
        private String state() {
            Editable text = getEditable();
            return Selection.getSelectionStart(text) + "," + Selection.getSelectionEnd(text)
                    + "," + getComposingSpanStart(text) + "," + getComposingSpanEnd(text)
                    + "," + text;
        }

        @Override
        public boolean requestCursorUpdates(int mode) {
            return updates.cursorUpdatesRequested(mode);
        }

        @Override
        public boolean requestCursorUpdates(int mode, int filter) {
            // What is told is told whole: the filter only narrows it.
            return updates.cursorUpdatesRequested(mode | filter);
        }

        @Override
        public boolean setSelection(int start, int end) {
            boolean set = super.setSelection(start, end);
            tell();
            return set;
        }

        @Override
        public boolean setComposingRegion(int start, int end) {
            composing = true;
            boolean set = super.setComposingRegion(start, end);
            composing = false;
            tell();
            return set;
        }

        // GameTextInput's own setSelection and setComposingRegion change
        // only its Editable; the end of a batch edit is where it tells its
        // listener the state, so close one to pass the new cursor on. Its
        // base class keeps no batch count, so an input method's own batch
        // is not cut short. It is closed once the call is done: the native
        // side's own state comes in through GameTextInput's final setState,
        // which sets the cursor through these same two calls and then
        // restarts the input, and that state told back late would undo a
        // newer edit. Done ahead of what is queued, so a key the input
        // method sends next finds the native side's cursor already moved.
        private void tell() {
            if (!moved) {
                moved = true;
                main.postAtFrontOfQueue(this::told);
            }
        }

        private void told() {
            if (moved) {
                endBatchEdit();
            }
        }

        @Override
        public boolean onKey(View view, int code, KeyEvent event) {
            keyed = true;
            boolean handled = super.onKey(view, code, event);
            keyed = false;
            if (handled) {
                reported();
            }
            return handled;
        }

        // A report was made, and the native side can read it by now. Read
        // after the listener returns: a write announced later is judged on
        // this report. Nothing is held for a write, so one that never lands
        // costs no report.
        private void reported() {
            overtaken = writing.get() > 0;
            overwrote = false;
        }

        // GameTextInput restarts at the end of setState, whose state is the
        // native side's, at the end of a key it handled, which it has told
        // itself, and on showing or hiding the keyboard. The first two are
        // what is known now and nothing is left to tell; a move made before
        // the third still is.
        @Override
        public void restartInput() {
            Editable text = getEditable();
            boolean written = text.getSpanStart(unwritten) < 0;
            if (written) {
                mark();
                // Set where a state is told, and a write is not.
                EditorInfo info = getEditorInfo();
                info.initialSelStart = Selection.getSelectionStart(text);
                info.initialSelEnd = Selection.getSelectionEnd(text);
            }
            if (written || keyed) {
                moved = false;
                String now = state();
                if (written) {
                    if (writing.get() > 0) {
                        writing.decrementAndGet();
                    }
                    // What lands is the newest write, whichever command
                    // brought it, so it is what the native side is to end on.
                    overwrote |= overtaken && !now.equals(known);
                    overtaken = false;
                }
                known = now;
                holdsWrite = written;
                if (written && overwrote) {
                    tell();
                }
            }
            super.restartInput();
        }

        // Only a state that differs from the last told or written is told,
        // and a write that overwrote a report, once.
        // A restart, this one's or LjosActivity's, closes the connection
        // and that ends a batch: told, a write would come back to the
        // native side as the method's report, which forgets the writes
        // still on their way. BaseInputConnection's own batch, setting a
        // composing region, is told by tell(), or not at all inside setState.
        @Override
        public boolean endBatchEdit() {
            if (composing) {
                return false;
            }
            moved = false;
            String now = state();
            if (now.equals(known)) {
                if (!overwrote) {
                    return false;
                }
                echoing = true;
                boolean more = super.endBatchEdit();
                echoing = false;
                reported();
                return more;
            }
            known = now;
            holdsWrite = false;
            boolean more = super.endBatchEdit();
            reported();
            return more;
        }

        // Closing finishes the method's composition. One the native side
        // wrote stays, as the native side holds it.
        @Override
        public boolean finishComposingText() {
            if (closing && holdsWrite && state().equals(known)) {
                return true;
            }
            return super.finishComposingText();
        }

        @Override
        public void closeConnection() {
            updates.cursorUpdatesRequested(0);
            closing = true;
            super.closeConnection();
            closing = false;
        }
    }
}
