package android.view.inputmethod;

import android.text.Editable;
import android.text.Selection;
import android.text.Spanned;

/** The documented behaviour of the calls the connection makes or overrides. */
public class BaseInputConnection {
    private static final Object COMPOSING = new Object();

    public Editable getEditable() {
        throw new UnsupportedOperationException();
    }

    public static int getComposingSpanStart(Editable text) {
        return text.getSpanStart(COMPOSING);
    }

    public static int getComposingSpanEnd(Editable text) {
        return text.getSpanEnd(COMPOSING);
    }

    public boolean requestCursorUpdates(int mode) {
        return false;
    }

    public boolean requestCursorUpdates(int mode, int filter) {
        return false;
    }

    public boolean beginBatchEdit() {
        return false;
    }

    public boolean endBatchEdit() {
        return false;
    }

    public boolean setSelection(int start, int end) {
        Editable text = getEditable();
        if (start >= 0 && end >= 0 && start <= text.length() && end <= text.length()) {
            Selection.setSelection(text, start, end);
        }
        return true;
    }

    public boolean setComposingRegion(int start, int end) {
        Editable text = getEditable();
        beginBatchEdit();
        text.removeSpan(COMPOSING);
        int from = Math.max(0, Math.min(start, end));
        int to = Math.min(text.length(), Math.max(start, end));
        if (from < to) {
            compose(text, from, to);
        }
        endBatchEdit();
        return true;
    }

    public boolean setComposingText(CharSequence with, int newCursorPosition) {
        return replace(with, newCursorPosition, true);
    }

    public boolean commitText(CharSequence with, int newCursorPosition) {
        return replace(with, newCursorPosition, false);
    }

    public boolean finishComposingText() {
        beginBatchEdit();
        getEditable().removeSpan(COMPOSING);
        endBatchEdit();
        return true;
    }

    public void closeConnection() {
        finishComposingText();
    }

    private static void compose(Editable text, int from, int to) {
        text.setSpan(COMPOSING, from, to,
                Spanned.SPAN_EXCLUSIVE_EXCLUSIVE | Spanned.SPAN_COMPOSING);
    }

    // Android's newCursorPosition: above 0 from the end of what was put
    // in, less one; else from its start.
    private boolean replace(CharSequence with, int newCursorPosition, boolean composing) {
        Editable text = getEditable();
        beginBatchEdit();
        int from = getComposingSpanStart(text);
        int to = getComposingSpanEnd(text);
        if (from < 0) {
            from = Math.max(0, Selection.getSelectionStart(text));
            to = Math.max(0, Selection.getSelectionEnd(text));
        }
        int start = Math.min(from, to);
        text.removeSpan(COMPOSING);
        text.replace(start, Math.max(from, to), with);
        int end = start + with.length();
        if (composing && with.length() > 0) {
            compose(text, start, end);
        }
        int cursor = newCursorPosition > 0 ? end + newCursorPosition - 1 : start + newCursorPosition;
        cursor = Math.max(0, Math.min(cursor, text.length()));
        Selection.setSelection(text, cursor, cursor);
        endBatchEdit();
        return true;
    }
}
