package android.text;

import java.util.IdentityHashMap;
import java.util.Map;

/** A text and the spans over it, as far as the connection uses them. */
public final class Editable implements Spanned {
    private final StringBuilder text = new StringBuilder();
    private final Map<Object, int[]> spans = new IdentityHashMap<>();

    public int length() {
        return text.length();
    }

    @Override
    public String toString() {
        return text.toString();
    }

    public void clear() {
        replace(0, length(), "");
    }

    public void clearSpans() {
        spans.clear();
    }

    public void insert(int at, CharSequence with) {
        replace(at, at, with);
    }

    public void replace(int from, int to, CharSequence with) {
        text.replace(from, to, with.toString());
        int by = with.length() - (to - from);
        spans.values().removeIf(s -> to > from && s[0] >= from && s[1] <= to
                && (s[2] & 0x33) == SPAN_EXCLUSIVE_EXCLUSIVE);
        for (int[] span : spans.values()) {
            for (int end = 0; end < 2; end++) {
                if (span[end] > from) {
                    span[end] = span[end] >= to ? span[end] + by : from;
                }
            }
        }
    }

    public void setSpan(Object what, int start, int end, int flags) {
        spans.put(what, new int[] {start, end, flags});
    }

    public void removeSpan(Object what) {
        spans.remove(what);
    }

    public int getSpanStart(Object what) {
        int[] span = spans.get(what);
        return span == null ? -1 : span[0];
    }

    public int getSpanEnd(Object what) {
        int[] span = spans.get(what);
        return span == null ? -1 : span[1];
    }
}
