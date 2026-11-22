package dev.ljos;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.Context;
import android.content.Intent;
import android.content.res.AssetFileDescriptor;
import android.database.Cursor;
import android.net.Uri;
import android.os.CancellationSignal;
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
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * The system's document picker (the Storage Access Framework), for the
 * application's native side (ljos_platform's AndroidDialogs), which shows a
 * file or folder dialog with it.
 *
 * <p>A copy is kept a week, or until the system clears the cache: its path
 * is no lasting name for the document.
 *
 * <p>A document opened is copied into the application's cache, on a thread
 * of its own, and its copy's path is the answer, so whatever reads a file
 * reads it. A document to save to, and a folder, are answered with the
 * provider's URI, whose grant is kept across restarts.
 *
 * <p>A document says nothing reliable of its size, and may never end: a
 * copy stops when the dialog is closed, also in a call to the provider that
 * waits, and leaves {@link #ROOM_LEFT} of the storage free. A provider that
 * can't be interrupted keeps only the thread: a close waits for none, and
 * takes the copies away itself.
 */
public final class LjosDocuments {
    private LjosDocuments() {}

    private static final int OPEN = 0;
    private static final int OPEN_MANY = 1;
    private static final int SAVE = 2;
    private static final int FOLDER = 3;

    /** What a copy leaves free where the cache is: the system's, and the application's to run on. */
    private static final long ROOM_LEFT = 64L * 1024 * 1024;

    /** How long a copy is kept in the cache, which the system may clear sooner. */
    private static final long KEPT_MS = 7L * 24 * 60 * 60 * 1000;

    /** The picker each request code on screen is. */
    private static final SparseIntArray showing = new SparseIntArray();

    /** A selection being copied: what its calls to the provider end by, and where it goes. */
    private static final class Copying {
        final CancellationSignal cancel = new CancellationSignal();
        final File dir;

        Copying(File dir) {
            this.dir = dir;
        }
    }

    /** The selections being copied, by request code; under {@code showing}. */
    private static final Map<Integer, Copying> copying = new HashMap<>();

    /**
     * The request codes closed while shown or copied, whose answer the native
     * side no longer waits for; under {@code showing}.
     */
    private static final Set<Integer> closed = new HashSet<>();

    /** The document each copy is reading, for a close to end; under {@code showing}. */
    private static final Map<Integer, InputStream> reading = new HashMap<>();

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
            // The code comes round again after 65535 pickers: a mark of its
            // last use is not this one's.
            closed.remove(code);
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
        InputStream read;
        Copying copy;
        synchronized (showing) {
            copy = copying.get(code);
            // A picker that has just answered, or a selection still being
            // copied, would answer no one: its copies are not left behind.
            if (showing.indexOfKey(code) >= 0 || copy != null) {
                closed.add(code);
            }
            read = reading.remove(code);
        }
        LjosActivity activity = LjosActivity.current();
        if (activity != null) {
            activity.runOnUiThread(() -> activity.finishActivity(code));
        }
        if (copy != null) {
            // Not on the application's thread: cancelling runs a provider's
            // own code when it is in this process, and closing can wait.
            new Thread(() -> end(copy, read), "ljos-documents-close").start();
        }
    }

    /**
     * Ends what a closed copy waits for, and first removes what it made,
     * which a provider that doesn't return would keep it from doing. Nothing
     * is made there after the mark ({@link #create}).
     */
    private static void end(Copying copy, InputStream read) {
        remove(copy.dir);
        // A read of a provider that sends nothing sees no mark: closing its
        // stream is what ends it.
        if (read != null) {
            try {
                read.close();
            } catch (IOException | RuntimeException e) {
                Log.w("ljos", "the document being read was not closed", e);
            }
        }
        try {
            copy.cancel.cancel();
        } catch (RuntimeException e) {
            Log.w("ljos", "the provider was not cancelled", e);
        }
    }

    /**
     * The activity's result (onActivityResult): when it is a picker's, what
     * the user chose goes to the native side.
     */
    static void answered(LjosActivity activity, int code, int result, Intent data) {
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
        Context context = activity.getApplicationContext();
        File copies = new File(context.getCacheDir(), "documents");
        // Codes start again in each process: the time keeps an earlier
        // run's copy from being replaced. Digits alone, as remove_copies
        // (android_documents.rs) takes a directory of copies to be.
        Copying copy = new Copying(new File(copies,
                String.format(java.util.Locale.ROOT, "%d%05d", System.currentTimeMillis(), code)));
        int picker;
        boolean wanted;
        // From shown to copied in one step: a close in between is marked.
        synchronized (showing) {
            picker = showing.get(code, -1);
            if (picker < 0) {
                return;
            }
            showing.delete(code);
            wanted = !closed.remove(code) && !uris.isEmpty();
            if (wanted && (picker == OPEN || picker == OPEN_MANY)) {
                copying.put(code, copy);
            }
        }
        if (!wanted) {
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
        new Thread(() -> {
            prune(copies);
            String chosen = copy(context, code, copy, uris);
            boolean unheard;
            synchronized (showing) {
                copying.remove(code);
                unheard = closed.remove(code);
            }
            // No answer after a close. One can come after that last look at
            // the mark: whether anyone took the answer, only the native side
            // knows.
            if (unheard || !nativeAnswered(code, chosen)) {
                remove(copy.dir);
            }
        }, "ljos-documents").start();
    }

    private static native boolean nativeAnswered(int code, String chosen);

    /** The picker shown with {@code code}, no longer shown; -1 if none was. */
    private static int forget(int code) {
        synchronized (showing) {
            int picker = showing.get(code, -1);
            showing.delete(code);
            return picker;
        }
    }

    /** Whether the dialog shown with {@code code} was closed since. */
    private static boolean unwanted(int code) {
        synchronized (showing) {
            return closed.contains(code);
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
     * there; else the first type offered, which the native side makes the
     * selected filter's.
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
     * Copies each document into the cache's {@code documents/<pick>/<n>/},
     * under its own name; the copies' paths, a line each, or null when one
     * could not be read or held, or the dialog was closed meanwhile. A selection that
     * fails, or is no longer wanted, leaves no copy behind: the application
     * is told of none, so nothing else would remove them.
     */
    private static String copy(Context context, int code, Copying copy, List<Uri> uris) {
        File dir = copy.dir;
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < uris.size(); i++) {
            Uri uri = uris.get(i);
            File into = new File(dir, Integer.toString(i));
            String name = name(context, uri, copy.cancel);
            if (name == null || unwanted(code)) {
                remove(dir);
                return null;
            }
            File file = new File(into, name);
            try (InputStream in = open(context, uri, copy.cancel)) {
                synchronized (showing) {
                    // Closed while the provider opened it: no close will
                    // find this stream to end.
                    if (closed.contains(code)) {
                        throw new IOException("the dialog was closed");
                    }
                    reading.put(code, in);
                }
                try (OutputStream to = create(code, into, file)) {
                    byte[] buffer = new byte[64 * 1024];
                    for (int n; (n = in.read(buffer)) > 0; ) {
                        if (context.getCacheDir().getUsableSpace() < ROOM_LEFT + n) {
                            throw new IOException("the storage has no room for the copy");
                        }
                        to.write(buffer, 0, n);
                        if (unwanted(code)) {
                            throw new IOException("the dialog was closed");
                        }
                    }
                } finally {
                    synchronized (showing) {
                        reading.remove(code);
                    }
                }
            } catch (IOException | RuntimeException e) {
                // A provider's own exception is thrown again here, in our
                // process: uncaught, it would end the application.
                Log.w("ljos", "the document " + uri + " was not read", e);
                remove(dir);
                return null;
            }
            out.append(file.getAbsolutePath()).append('\n');
        }
        return out.toString();
    }

    /** The name the provider gives a document, as a file's name. */
    private static String name(Context context, Uri uri, CancellationSignal cancel) {
        String name = null;
        try (Cursor c = context.getContentResolver().query(
                uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null, cancel)) {
            if (c != null && c.moveToFirst() && !c.isNull(0)) {
                name = c.getString(0);
            }
        } catch (RuntimeException e) {
            Log.w("ljos", "no name for " + uri, e);
        }
        if (cancel.isCanceled()) {
            return null;
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
    /**
     * The document to read, as openInputStream gives it, which has no way to
     * be cancelled; openFileDescriptor would refuse a part of a file.
     */
    private static InputStream open(Context context, Uri uri, CancellationSignal cancel)
            throws IOException {
        AssetFileDescriptor document =
                context.getContentResolver().openAssetFileDescriptor(uri, "r", cancel);
        if (document == null) {
            throw new IOException("the provider gave nothing to read");
        }
        try {
            return document.createInputStream();
        } catch (IOException e) {
            document.close();
            throw e;
        }
    }

    /**
     * The file to copy into, made under the lock that marks a close: what
     * exists was made before the mark, so the close removes it.
     */
    private static OutputStream create(int code, File into, File file) throws IOException {
        synchronized (showing) {
            if (closed.contains(code)) {
                throw new IOException("the dialog was closed");
            }
            if (!into.mkdirs() && !into.isDirectory()) {
                throw new IOException("no directory for the copy: " + into);
            }
            return new FileOutputStream(file);
        }
    }

    /** Removes the directories of copies made longer ago than they are kept. */
    private static void prune(File copies) {
        File[] made = copies.listFiles();
        if (made == null) {
            return;
        }
        long now = System.currentTimeMillis();
        for (File dir : made) {
            if (dir.isDirectory() && dir.getName().matches("[0-9]+")
                    && now - dir.lastModified() > KEPT_MS) {
                remove(dir);
            }
        }
    }

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
