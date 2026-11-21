package com.google.androidgamesdk;

import com.google.androidgamesdk.gametextinput.InputConnection;

/**
 * The connection GameTextInput keeps for the surface's editor. GameActivity
 * keeps it to its own package; LjosActivity needs it to type what a dead key
 * composes into the field being edited, the way an input method would.
 */
public final class TextInputOf {
    private TextInputOf() {}

    /** The surface's connection, or null before an input method asked for one. */
    public static InputConnection connection(GameActivity.InputEnabledSurfaceView view) {
        return view == null ? null : view.mInputConnection;
    }
}
