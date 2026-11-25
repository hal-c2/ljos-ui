package dev.ljos;

import android.content.Context;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.os.Build;
import android.util.Log;
import android.util.SparseArray;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * The system's runtime permissions, for the application's native side
 * (ljos_platform's AndroidPermissions). What is asked comes as stages, a
 * line each, of {@code name@since} apart by spaces: the API level that made
 * the permission a runtime one, before which the system granted it at
 * install. A stage is asked once the one before it is granted (the
 * background location, since Android 11, only after the location in use).
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
            if (since(name) > Build.VERSION.SDK_INT) {
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
     * {@code code}, at once when there is nothing the system would ask.
     * False when no activity is there to ask from.
     */
    public static boolean request(Context context, String stages, int code) {
        LjosActivity activity = LjosActivity.current();
        if (activity == null) {
            return false;
        }
        List<String[]> left = new ArrayList<>();
        for (String stage : stages.split("\n")) {
            left.add(names(stage, true).toArray(new String[0]));
        }
        synchronized (asking) {
            asking.put(code, left);
            questions.put(code, stages);
        }
        activity.runOnUiThread(() -> next(activity, code));
        return true;
    }

    /**
     * Asks the next stage of {@code code}, or answers when none is left or
     * the stage can't be granted.
     */
    private static void next(LjosActivity activity, int code) {
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
        if (stage == null) {
            answer(code, states(activity, question));
            return;
        }
        // Asked again, a permission the user denied may be granted: the
        // system decides whether it shows its question once more. One the
        // manifest does not declare it would deny without a word.
        Set<String> declared = declared(activity);
        List<String> ask = new ArrayList<>();
        for (String name : stage) {
            if (!declared.contains(name)) {
                answer(code, states(activity, question));
                return;
            }
            if (activity.checkSelfPermission(name) != PackageManager.PERMISSION_GRANTED) {
                ask.add(name);
            }
        }
        if (ask.isEmpty()) {
            next(activity, code);
            return;
        }
        activity.requestPermissions(ask.toArray(new String[0]), code);
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
        if (names.length == 0 || results.length == 0) {
            answer(code, null);
            return;
        }
        SharedPreferences.Editor asked =
                activity.getSharedPreferences(ASKED, Context.MODE_PRIVATE).edit();
        for (String name : names) {
            asked.putBoolean(name, true);
        }
        asked.apply();
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
        next(activity, code);
    }

    private static void answer(int code, String states) {
        synchronized (asking) {
            asking.remove(code);
            questions.remove(code);
        }
        nativeAnswered(code, states);
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
            } else if (since(name) <= Build.VERSION.SDK_INT) {
                out.add(bare(name));
            }
        }
        return out;
    }

    private static String bare(String name) {
        int at = name.indexOf('@');
        return at < 0 ? name : name.substring(0, at);
    }

    private static int since(String name) {
        int at = name.indexOf('@');
        try {
            return at < 0 ? 0 : Integer.parseInt(name.substring(at + 1));
        } catch (NumberFormatException e) {
            return 0;
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
