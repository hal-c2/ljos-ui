package com.google.androidgamesdk;

import android.content.Context;
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

    /**
     * Runs {@code push}, which sets the state the native side asked for,
     * without telling the native side that state back: GameTextInput sets
     * it through the same setSelection and setComposingRegion an input
     * method calls, and an echo arriving late would undo a newer edit.
     */
    public static void quietly(GameActivity.InputEnabledSurfaceView view, Runnable push) {
        InputConnection connection = connection(view);
        if (!(connection instanceof Connection)) {
            push.run();
            return;
        }
        Connection ours = (Connection) connection;
        Thread before = ours.pushing;
        ours.pushing = Thread.currentThread();
        try {
            push.run();
        } finally {
            ours.pushing = before;
        }
    }

    private static final class Connection extends InputConnection {
        private final CursorUpdates updates;

        /** The thread setting a state from the native side, or null. */
        volatile Thread pushing;

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
            boolean set = super.setComposingRegion(start, end);
            tell();
            return set;
        }

        // GameTextInput's own setSelection and setComposingRegion change
        // only its Editable; the end of a batch edit is where it tells its
        // listener the state, so close one to pass the new cursor on. Its
        // base class keeps no batch count, so an input method's own batch
        // is not cut short.
        private void tell() {
            if (pushing != Thread.currentThread()) {
                endBatchEdit();
            }
        }

        @Override
        public void closeConnection() {
            updates.cursorUpdatesRequested(0);
            super.closeConnection();
        }
    }
}
