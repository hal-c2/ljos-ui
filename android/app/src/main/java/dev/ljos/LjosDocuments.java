package dev.ljos;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;
import android.util.Log;
import android.util.SparseIntArray;
import android.webkit.MimeTypeMap;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;

/**
 * The system's document picker (the Storage Access Framework), for the
 * application's native side (ljos_platform's AndroidDialogs), which shows a
 * file or folder dialog with it.
 *
 * <p>A document opened is copied into the application's cache, on a thread
 * of its own, and its copy's path is the answer, so whatever reads a file
 * reads it. A document to save to, and a folder, are answered with the
 * provider's URI, whose grant is kept across restarts.
 */
public final class LjosDocuments {
    private LjosDocuments() {}

    private static final int OPEN = 0;
    private static final int OPEN_MANY = 1;
    private static final int SAVE = 2;
    private static final int FOLDER = 3;

    /** The picker each request code on screen is. */
    private static final SparseIntArray showing = new SparseIntArray();

    /**
     * Shows a picker from the activity on screen; its answer comes to the
     * native side with {@code code}. {@code extensions} are the ones offered,
     * a line each, or empty for every document; {@code name} is the name a
     * save suggests. False when no activity is there to show it from.
     */
    public static boolean show(Context context, int picker, String extensions, String name, int code) {
        LjosActivity activity = LjosActivity.current();
        if (activity == null) {
            return false;
        }
        Intent intent;
        String[] types = types(extensions);
        switch (picker) {
            case OPEN:
            case OPEN_MANY:
                intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
                offer(intent, types);
                intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, picker == OPEN_MANY);
                break;
            case SAVE:
                intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
                intent.setType(saved(name, types));
                if (!name.isEmpty()) {
                    intent.putExtra(Intent.EXTRA_TITLE, name);
                }
                break;
            case FOLDER:
                intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
                break;
            default:
                return false;
        }
        synchronized (showing) {
            showing.put(code, picker);
        }
        Intent asked = intent;
        activity.runOnUiThread(() -> {
            try {
                activity.startActivityForResult(asked, code);
            } catch (ActivityNotFoundException e) {
                Log.w("ljos", "no document picker on this system", e);
                if (forget(code) >= 0) {
                    nativeAnswered(code, null);
                }
            }
        });
        return true;
    }

    /** Takes away the picker shown with {@code code}, if it is still up. */
    public static void close(int code) {
        LjosActivity activity = LjosActivity.current();
        if (activity != null) {
            activity.runOnUiThread(() -> activity.finishActivity(code));
        }
    }

    /**
     * The activity's result (onActivityResult): when it is a picker's, what
     * the user chose goes to the native side.
     */
    static void answered(LjosActivity activity, int code, int result, Intent data) {
        int picker = forget(code);
        if (picker < 0) {
            return;
        }
        List<Uri> uris = new ArrayList<>();
        if (result == Activity.RESULT_OK && data != null) {
            ClipData many = data.getClipData();
            if (many != null) {
                for (int i = 0; i < many.getItemCount(); i++) {
                    uris.add(many.getItemAt(i).getUri());
                }
            } else if (data.getData() != null) {
                uris.add(data.getData());
            }
        }
        if (uris.isEmpty()) {
            nativeAnswered(code, null);
            return;
        }
        if (picker == SAVE || picker == FOLDER) {
            StringBuilder out = new StringBuilder();
            for (Uri uri : uris) {
                keep(activity, uri);
                out.append(uri.toString()).append('\n');
            }
            nativeAnswered(code, out.toString());
            return;
        }
        Context context = activity.getApplicationContext();
        new Thread(() -> nativeAnswered(code, copy(context, code, uris)), "ljos-documents").start();
    }

    /**
     * What the user chose, to ljos_platform: a line each, the path of a copy
     * or the provider's URI; null when the user chose nothing.
     */
    private static native void nativeAnswered(int code, String chosen);

    /** The picker shown with {@code code}, no longer shown; -1 if none was. */
    private static int forget(int code) {
        synchronized (showing) {
            int picker = showing.get(code, -1);
            showing.delete(code);
            return picker;
        }
    }

    /** The MIME types of {@code extensions}, or null to offer every document. */
    private static String[] types(String extensions) {
        List<String> out = new ArrayList<>();
        MimeTypeMap map = MimeTypeMap.getSingleton();
        for (String ext : extensions.split("\n")) {
            if (ext.isEmpty()) {
                continue;
            }
            String type = map.getMimeTypeFromExtension(ext);
            // One the system does not know of: the user may still pick it.
            if (type == null) {
                return null;
            }
            if (!out.contains(type)) {
                out.add(type);
            }
        }
        return out.isEmpty() ? null : out.toArray(new String[0]);
    }

    /**
     * The type of the document a save creates, which the provider keeps:
     * the suggested name's, as the application names the format it writes
     * there; else the first filter's, the dialog having no other choice of
     * format to go by.
     */
    private static String saved(String name, String[] types) {
        int dot = name.lastIndexOf('.');
        if (dot > 0) {
            String type = MimeTypeMap.getSingleton()
                    .getMimeTypeFromExtension(name.substring(dot + 1).toLowerCase(java.util.Locale.ROOT));
            if (type != null) {
                return type;
            }
        }
        return types == null ? "application/octet-stream" : types[0];
    }

    private static void offer(Intent intent, String[] types) {
        if (types == null) {
            intent.setType("*/*");
        } else if (types.length == 1) {
            intent.setType(types[0]);
        } else {
            intent.setType("*/*");
            intent.putExtra(Intent.EXTRA_MIME_TYPES, types);
        }
    }

    /** Keeps the grant of a document or folder across restarts. */
    private static void keep(Context context, Uri uri) {
        try {
            context.getContentResolver().takePersistableUriPermission(
                    uri, Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
        } catch (SecurityException e) {
            Log.w("ljos", "the grant of " + uri + " lasts only while the application runs", e);
        }
    }

    /**
     * Copies each document into the cache's {@code documents/<code>/<n>/},
     * under its own name; the copies' paths, a line each, or null when one
     * could not be read. A selection that fails leaves no copy behind: the
     * application is told of none, so nothing else would remove them.
     */
    private static String copy(Context context, int code, List<Uri> uris) {
        File dir = new File(context.getCacheDir(), "documents/" + code);
        remove(dir);
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < uris.size(); i++) {
            Uri uri = uris.get(i);
            File into = new File(dir, Integer.toString(i));
            File file = new File(into, name(context, uri));
            if (!into.mkdirs() && !into.isDirectory()) {
                Log.w("ljos", "no directory for the copy of " + uri);
                remove(dir);
                return null;
            }
            try (InputStream in = context.getContentResolver().openInputStream(uri);
                    OutputStream to = new FileOutputStream(file)) {
                if (in == null) {
                    throw new IOException("the provider gave nothing to read");
                }
                byte[] buffer = new byte[64 * 1024];
                for (int n; (n = in.read(buffer)) > 0; ) {
                    to.write(buffer, 0, n);
                }
            } catch (IOException | SecurityException e) {
                Log.w("ljos", "the document " + uri + " was not read", e);
                remove(dir);
                return null;
            }
            out.append(file.getAbsolutePath()).append('\n');
        }
        return out.toString();
    }

    /** The name the provider gives a document, as a file's name. */
    private static String name(Context context, Uri uri) {
        String name = null;
        try (Cursor c = context.getContentResolver().query(
                uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst() && !c.isNull(0)) {
                name = c.getString(0);
            }
        } catch (RuntimeException e) {
            Log.w("ljos", "no name for " + uri, e);
        }
        if (name == null) {
            name = uri.getLastPathSegment();
        }
        // The answer to the native side is a line per document: a name
        // keeps to one line, and to one path segment.
        name = name == null
                ? ""
                : name.replace('/', '_').replace('\0', '_').replace('\n', '_').replace('\r', '_');
        return name.isEmpty() || name.equals(".") || name.equals("..") ? "document" : name;
    }

    /** Removes an earlier picker's copies, which a code used again replaces. */
    private static void remove(File file) {
        File[] inside = file.listFiles();
        if (inside != null) {
            for (File f : inside) {
                remove(f);
            }
        }
        if (file.exists() && !file.delete()) {
            Log.w("ljos", "an earlier copy stays: " + file);
        }
    }
}
