package com.google.androidgamesdk.gametextinput;

public final class State {
    public final String text;
    public final int selectionStart;
    public final int selectionEnd;
    public final int composingRegionStart;
    public final int composingRegionEnd;

    public State(String text, int selectionStart, int selectionEnd, int composingRegionStart,
            int composingRegionEnd) {
        this.text = text;
        this.selectionStart = selectionStart;
        this.selectionEnd = selectionEnd;
        this.composingRegionStart = composingRegionStart;
        this.composingRegionEnd = composingRegionEnd;
    }

    @Override
    public String toString() {
        return text + "|" + selectionStart + "," + selectionEnd + "|" + composingRegionStart + ","
                + composingRegionEnd;
    }
}
