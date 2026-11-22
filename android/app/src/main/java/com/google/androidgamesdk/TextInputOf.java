package com.google.androidgamesdk;

import android.content.Context;
import android.os.Handler;
import android.os.Looper;
import android.view.View;
import android.view.inputmethod.EditorInfo;
import com.google.androidgamesdk.gametextinput.InputConnection;
import com.google.androidgamesdk.gametextinput.Settings;

/**
 * The connection GameTextInput keeps for the surface's editor. GameActivity
 * keeps it to its own package; LjosActivity needs it to type what a dead key
 * composes into the field being edited, the way an input method would, and
 * to hear an input method ask where the text is on screen, which
 * GameTextInput's own connection refuses, and to pass on a selection or
 * composing region an input method sets without changing the text, which
 * GameTextInput's own connection keeps to its Editable.
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

        Connection(Context context, View view, Settings settings, CursorUpdates updates) {
            super(context, view, settings);
            this.updates = updates;
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

        // GameTextInput restarts at the end of setState, whose state is the
        // native side's, and at the end of a key it handled, which it has
        // reported itself: neither is told again.
        @Override
        public void restartInput() {
            moved = false;
            super.restartInput();
        }

        // BaseInputConnection's own, setting a composing region, is told
        // by tell(), or not at all inside setState.
        @Override
        public boolean endBatchEdit() {
            if (composing) {
                return false;
            }
            moved = false;
            return super.endBatchEdit();
        }

        @Override
        public void closeConnection() {
            updates.cursorUpdatesRequested(0);
            super.closeConnection();
        }
    }
}
