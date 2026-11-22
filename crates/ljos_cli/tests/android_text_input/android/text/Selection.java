package android.text;

public final class Selection {
    private static final Object START = new Object();
    private static final Object END = new Object();

    public static void setSelection(Editable text, int start, int end) {
        text.setSpan(START, start, start, Spanned.SPAN_POINT_POINT);
        text.setSpan(END, end, end, Spanned.SPAN_POINT_POINT);
    }

    public static int getSelectionStart(Editable text) {
        return text.getSpanStart(START);
    }

    public static int getSelectionEnd(Editable text) {
        return text.getSpanStart(END);
    }
}
