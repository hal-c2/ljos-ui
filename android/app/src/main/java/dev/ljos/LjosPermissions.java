package dev.ljos;

import android.content.Context;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.util.SparseArray;
import java.lang.ref.WeakReference;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * The system's runtime permissions, for the application's native side
 * (ljos_platform's AndroidPermissions). What is asked comes as stages, a
 * line each, of {@code name@since} or {@code name@since-until} apart by
 * spaces: the API level that made the permission a runtime one, before
 * which the system granted it at install, and the last that has it, after
 * which nothing asks for it. A stage is asked once the one before it is
 * granted (the background location, since Android 11, only after the
 * location in use).
 *
 * <p>The system asks one question at a time: a request made while another
 * is shown would be cut short. A request waits for the ones before it
 * ({@link #waiting}).
 *
 * <p>The system says only whether a permission is granted; one it does
 * not grant is undetermined until the application has asked for it, and
 * denied after. What has been asked is kept in the application's own
 * preferences ({@link #ASKED}).
 */
public final class LjosPermissions {
    /** The preferences that hold what has been asked for. */
    static final String ASKED = "dev.ljos.permissions";

    /** The stages still to ask for, by the request's code. */
    private static final SparseArray<List<String[]>> asking = new SparseArray<>();

    /** The whole question, by the request's code, for its answer. */
    private static final SparseArray<String> questions = new SparseArray<>();

    /** The request being asked, or 0; codes start at 1. */
    private static int asked;

    /** Requests made while another was asked, in order. */
    private static final ArrayDeque<Integer> waiting = new ArrayDeque<>();

    /** The activity the system's question was shown from. */
    private static WeakReference<LjosActivity> shownFrom = new WeakReference<>(null);

    private static final Handler main = new Handler(Looper.getMainLooper());

    private LjosPermissions() {}

    private static native void nativeAnswered(int code, String states);

    /**
     * What the system says of each of the permissions, in order: {@code g}
     * granted, {@code d} asked for and not granted, or not declared in the
     * manifest, {@code u} never asked for.
     */
    public static String states(Context context, String stages) {
        Set<String> declared = declared(context);
        SharedPreferences asked = context.getSharedPreferences(ASKED, Context.MODE_PRIVATE);
        StringBuilder out = new StringBuilder();
        for (String name : names(stages, false)) {
            if (!applies(name)) {
                out.append('g');
            } else if (!declared.contains(bare(name))) {
                Log.w("ljos", bare(name) + " is not declared in the manifest: it is denied");
                out.append('d');
            } else if (context.checkSelfPermission(bare(name)) == PackageManager.PERMISSION_GRANTED) {
                out.append('g');
            } else {
                out.append(asked.getBoolean(bare(name), false) ? 'd' : 'u');
            }
        }
        return out.toString();
    }

    /**
     * Asks the user for what the system does not grant yet, from the
     * activity on screen; the answer comes to the native side with
     * {@code code}, at once when there is nothing the system would ask, and
     * after the requests made before it are answered. False when no
     * activity is there to ask from.
     */
    public static boolean request(Context context, String stages, int code) {
        if (LjosActivity.current() == null) {
            return false;
        }
        List<String[]> left = new ArrayList<>();
        for (String stage : stages.split("\n")) {
            left.add(names(stage, true).toArray(new String[0]));
        }
        boolean first;
        synchronized (asking) {
            asking.put(code, left);
            questions.put(code, stages);
            first = asked == 0;
            if (first) {
                asked = code;
            } else {
                waiting.add(code);
            }
        }
        if (first) {
            main.post(() -> next(code));
        }
        return true;
    }

    /**
     * Asks the next stage of {@code code}, or answers when none is left or
     * the stage can't be granted. On the main thread.
     */
    private static void next(int code) {
        String[] stage;
        String question;
        synchronized (asking) {
            List<String[]> left = asking.get(code);
            question = questions.get(code);
            stage = left == null || left.isEmpty() ? null : left.remove(0);
        }
        if (question == null) {
            return;
        }
        // The activity on screen now: the one the request was made from
        // may have been made again since, or be gone.
        LjosActivity activity = LjosActivity.current();
        if (activity == null) {
            answer(code, null);
            return;
        }
        if (stage == null) {
            answer(code, states(activity, question));
            return;
        }
        // Asked again, a permission the user denied may be granted: the
        // system decides whether it shows its question once more. One the
        // manifest does not declare it would deny without a word.
        Set<String> declared = declared(activity);
        boolean granted = true;
        for (String name : stage) {
            if (!declared.contains(name)) {
                answer(code, states(activity, question));
                return;
            }
            granted &= activity.checkSelfPermission(name) == PackageManager.PERMISSION_GRANTED;
        }
        if (granted) {
            next(code);
            return;
        }
        // The whole stage, what is granted too: Android 12 upgrades the
        // approximate location to the precise one only when both are asked.
        shownFrom = new WeakReference<>(activity);
        activity.requestPermissions(stage, code);
    }

    /**
     * The activity is gone for good, not made again: the question it showed
     * is not answered to it, and so is cut short.
     */
    static void ended(LjosActivity activity) {
        int code;
        synchronized (asking) {
            code = shownFrom.get() == activity ? asked : 0;
        }
        if (code != 0) {
            answer(code, null);
        }
    }

    /**
     * The activity's onRequestPermissionsResult: the user answered the
     * stage asked with {@code code}. Nothing is said for a question cut
     * short, which the native side hears as no answer.
     */
    static void answered(LjosActivity activity, int code, String[] names, int[] results) {
        boolean known;
        synchronized (asking) {
            known = questions.get(code) != null;
        }
        if (!known) {
            return;
        }
        shownFrom = new WeakReference<>(null);
        if (names.length == 0 || results.length == 0) {
            answer(code, null);
            return;
        }
        SharedPreferences.Editor asked =
                activity.getSharedPreferences(ASKED, Context.MODE_PRIVATE).edit();
        for (String name : names) {
            asked.putBoolean(name, true);
        }
        // Written now, not later on a thread of its own: a process that
        // quits ends with _exit (ljos_shell's end_process), which waits on
        // no write still queued, and an asked name lost would read as never
        // asked on the next launch.
        asked.commit();
        // The next stage only once this one is granted.
        for (int result : results) {
            if (result != PackageManager.PERMISSION_GRANTED) {
                String question;
                synchronized (asking) {
                    question = questions.get(code);
                }
                answer(code, states(activity, question));
                return;
            }
        }
        next(code);
    }

    /** Answers {@code code}, and asks the request waiting after it. */
    private static void answer(int code, String states) {
        int following = 0;
        synchronized (asking) {
            asking.remove(code);
            questions.remove(code);
            if (asked == code) {
                Integer queued = waiting.poll();
                asked = queued == null ? 0 : queued;
                following = asked;
                shownFrom = new WeakReference<>(null);
            } else {
                waiting.remove(code);
            }
        }
        nativeAnswered(code, states);
        if (following != 0) {
            int then = following;
            main.post(() -> next(then));
        }
    }

    /**
     * The permissions of {@code stages}, as {@code name@since}; when
     * {@code asked}, only those the system asks for here, by name.
     */
    private static List<String> names(String stages, boolean asked) {
        List<String> out = new ArrayList<>();
        for (String name : stages.split("\\s+")) {
            if (name.isEmpty()) {
                continue;
            }
            if (!asked) {
                out.add(name);
            } else if (applies(name)) {
                out.add(bare(name));
            }
        }
        return out;
    }

    private static String bare(String name) {
        int at = name.indexOf('@');
        return at < 0 ? name : name.substring(0, at);
    }

    /** Whether this version of Android has the permission {@code name@since-until}. */
    private static boolean applies(String name) {
        int at = name.indexOf('@');
        if (at < 0) {
            return true;
        }
        String levels = name.substring(at + 1);
        int dash = levels.indexOf('-');
        int since = level(dash < 0 ? levels : levels.substring(0, dash), 0);
        int until = dash < 0 ? Integer.MAX_VALUE : level(levels.substring(dash + 1), Integer.MAX_VALUE);
        return since <= Build.VERSION.SDK_INT && Build.VERSION.SDK_INT <= until;
    }

    private static int level(String text, int otherwise) {
        try {
            return Integer.parseInt(text);
        } catch (NumberFormatException e) {
            return otherwise;
        }
    }

    /** The permissions the application's manifest declares. */
    private static Set<String> declared(Context context) {
        try {
            PackageInfo info = context.getPackageManager()
                    .getPackageInfo(context.getPackageName(), PackageManager.GET_PERMISSIONS);
            return info.requestedPermissions == null
                    ? new HashSet<>()
                    : new HashSet<>(Arrays.asList(info.requestedPermissions));
        } catch (PackageManager.NameNotFoundException e) {
            return new HashSet<>();
        }
    }
}
