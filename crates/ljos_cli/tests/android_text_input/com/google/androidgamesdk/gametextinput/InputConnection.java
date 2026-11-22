package com.google.androidgamesdk.gametextinput;

import android.content.Context;
import android.text.Editable;
import android.text.Selection;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputMethodManager;

/**
 * games-activity 4.4.0's order of calls among the methods TextInputOf
 * overrides or is reached through. Its one key is a left arrow.
 */
public class InputConnection extends BaseInputConnection {
    private final Settings settings;
    private final Editable editable = new Editable();
    private Listener listener;
    private boolean active;

    public InputConnection(Context context, View view, Settings settings) {
        this.settings = settings;
    }

    @Override
    public Editable getEditable() {
        return editable;
    }

    public final EditorInfo getEditorInfo() {
        return settings.editorInfo;
    }

    public final InputConnection setListener(Listener listener) {
        this.listener = listener;
        return this;
    }

    public final void setState(State state) {
        editable.clear();
        editable.clearSpans();
        editable.insert(0, state.text);
        setSelection(state.selectionStart, state.selectionEnd);
        if (state.composingRegionStart != state.composingRegionEnd) {
            setComposingRegion(state.composingRegionStart, state.composingRegionEnd);
        }
        restartInput();
    }

    public final void setSoftKeyboardActive(boolean active, int flags) {
        this.active = active;
        restartInput();
    }

    public void restartInput() {
        InputMethodManager.restartInput(this);
    }

    public boolean onKey(View view, int code, KeyEvent event) {
        if (!active) {
            return false;
        }
        int to = Math.max(0, Selection.getSelectionStart(editable) - 1);
        setSelection(to, to);
        stateUpdated();
        restartInput();
        return true;
    }

    @Override
    public boolean endBatchEdit() {
        stateUpdated();
        return super.endBatchEdit();
    }

    private void stateUpdated() {
        State state = new State(editable.toString(), Selection.getSelectionStart(editable),
                Selection.getSelectionEnd(editable), getComposingSpanStart(editable),
                getComposingSpanEnd(editable));
        settings.editorInfo.initialSelStart = state.selectionStart;
        settings.editorInfo.initialSelEnd = state.selectionEnd;
        if (listener != null) {
            listener.stateChanged(state, false);
        }
    }
}
