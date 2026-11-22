package com.google.androidgamesdk.gametextinput;

import android.view.inputmethod.EditorInfo;

public final class Settings {
    final EditorInfo editorInfo;

    public Settings(EditorInfo editorInfo, boolean forwardKeyEvents) {
        this.editorInfo = editorInfo;
    }
}
