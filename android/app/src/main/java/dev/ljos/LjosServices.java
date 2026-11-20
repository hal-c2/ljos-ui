package dev.ljos;

import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.GeneralSecurityException;
import java.security.KeyStore;
import java.security.MessageDigest;
import java.util.concurrent.Callable;
import java.util.concurrent.FutureTask;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/**
 * The system's services, for the application's native side (ljos_platform,
 * ljos_secrets), which calls these from a thread of its own through JNI.
 * Every method is static and takes the application's Context; what fails
 * throws, and the native side takes the exception as the error.
 */
public final class LjosServices {
    private LjosServices() {}

    /** How long the native side waits for the main thread. */
    private static final long MAIN_TIMEOUT_MS = 2000;

    /** Runs {@code call} on the main thread and waits for it. */
    private static <T> T onMain(Callable<T> call) throws Exception {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return call.call();
        }
        FutureTask<T> task = new FutureTask<>(call);
        new Handler(Looper.getMainLooper()).post(task);
        try {
            return task.get(MAIN_TIMEOUT_MS, TimeUnit.MILLISECONDS);
        } catch (TimeoutException e) {
            // Not run later, when the caller has its error: a clipboard
            // set after the caller was told it was not would surprise it.
            task.cancel(false);
            throw e;
        }
    }

    // The clipboard. Android lets only the application with focus read it,
    // and its listeners and its manager belong to the main thread.

    /** The clipboard's text, or null when it holds none. */
    public static String clipboardText(Context context) throws Exception {
        ClipData.Item item = onMain(() -> {
            ClipboardManager clipboard = context.getSystemService(ClipboardManager.class);
            ClipData clip = clipboard == null ? null : clipboard.getPrimaryClip();
            return clip == null || clip.getItemCount() == 0 ? null : clip.getItemAt(0);
        });
        if (item == null) {
            return null;
        }
        // On the caller's thread, not the main one: an item that is a
        // content URI is read from its provider, which may take its time,
        // and the main thread waiting on it would not answer the user.
        CharSequence text = item.coerceToText(context);
        return text == null ? null : text.toString();
    }

    /** Puts {@code text} on the clipboard. */
    public static void setClipboardText(Context context, String text) throws Exception {
        onMain(() -> {
            ClipboardManager clipboard = context.getSystemService(ClipboardManager.class);
            if (clipboard == null) {
                throw new IllegalStateException("no clipboard service");
            }
            clipboard.setPrimaryClip(ClipData.newPlainText(null, text));
            return null;
        });
    }

    // Links.

    /** Opens {@code url} in the application the user picked for it. */
    public static boolean openUrl(Context context, String url) {
        Intent view = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
        // The Context may be the application's, which has no task.
        view.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try {
            context.startActivity(view);
            return true;
        } catch (ActivityNotFoundException | SecurityException e) {
            return false;
        }
    }

    // The application's own directories: no other application reads them,
    // and the system removes them with the application.

    public static String filesDir(Context context) {
        return context.getFilesDir().getAbsolutePath();
    }

    public static String cacheDir(Context context) {
        return context.getCacheDir().getAbsolutePath();
    }

    // Secrets. Each is a file of its own in the application's no-backup
    // directory, encrypted with AES-GCM under a key that lives in the
    // Android Keystore and never leaves it: a copy of the files is nothing
    // without the device. The file is named by a digest of the service and
    // the key, which are also the cipher's associated data, so a file put
    // in another's place does not decrypt.

    private static final String KEYSTORE = "AndroidKeyStore";
    private static final String ALIAS = "ljos.secrets";
    private static final String CIPHER = "AES/GCM/NoPadding";
    private static final byte VERSION = 1;
    private static final int IV_LENGTH = 12;
    private static final int TAG_BITS = 128;

    // Synchronized: two threads that both found no key would each make one,
    // and the second would replace the key the first sealed a secret with.
    private static synchronized SecretKey key() throws GeneralSecurityException, IOException {
        KeyStore store = KeyStore.getInstance(KEYSTORE);
        store.load(null);
        KeyStore.Entry entry = store.getEntry(ALIAS, null);
        if (entry instanceof KeyStore.SecretKeyEntry) {
            return ((KeyStore.SecretKeyEntry) entry).getSecretKey();
        }
        KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE);
        generator.init(new KeyGenParameterSpec.Builder(
                        ALIAS, KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build());
        return generator.generateKey();
    }

    private static byte[] name(String service, String key) {
        // Neither holds a NUL (ljos_secrets checks), so the pair is one name.
        return (service + "\0" + key).getBytes(StandardCharsets.UTF_8);
    }

    private static File secretFile(Context context, byte[] name) throws GeneralSecurityException {
        byte[] digest = MessageDigest.getInstance("SHA-256").digest(name);
        StringBuilder hex = new StringBuilder(digest.length * 2);
        for (byte b : digest) {
            hex.append(String.format("%02x", b));
        }
        return new File(new File(context.getNoBackupFilesDir(), "ljos-secrets"), hex.toString());
    }

    /** Keeps {@code secret} as {@code key} of {@code service}. */
    public static void secretWrite(Context context, String service, String key, byte[] secret)
            throws GeneralSecurityException, IOException {
        byte[] name = name(service, key);
        Cipher cipher = Cipher.getInstance(CIPHER);
        // The Keystore picks the IV: one never used with this key.
        cipher.init(Cipher.ENCRYPT_MODE, key());
        cipher.updateAAD(name);
        byte[] sealed = cipher.doFinal(secret);
        byte[] iv = cipher.getIV();
        if (iv.length != IV_LENGTH) {
            throw new GeneralSecurityException("unexpected IV length " + iv.length);
        }
        File file = secretFile(context, name);
        File dir = file.getParentFile();
        // Another thread may make it between the two looks.
        if (!dir.isDirectory() && !dir.mkdirs() && !dir.isDirectory()) {
            throw new IOException("cannot make " + dir);
        }
        // Written whole beside it and moved over it: a secret is never half
        // written, nor lost to a write that did not finish.
        File part = File.createTempFile("part", null, dir);
        try (FileOutputStream out = new FileOutputStream(part)) {
            out.write(VERSION);
            out.write(iv);
            out.write(sealed);
            out.getFD().sync();
        } catch (IOException e) {
            part.delete();
            throw e;
        }
        if (!part.renameTo(file)) {
            part.delete();
            throw new IOException("cannot replace " + file);
        }
    }

    /** The secret kept as {@code key} of {@code service}, or null. */
    public static byte[] secretRead(Context context, String service, String key)
            throws GeneralSecurityException, IOException {
        byte[] name = name(service, key);
        File file = secretFile(context, name);
        if (!file.isFile()) {
            return null;
        }
        byte[] kept = Files.readAllBytes(file.toPath());
        if (kept.length < 1 + IV_LENGTH || kept[0] != VERSION) {
            throw new GeneralSecurityException("a secret's file is not one this writes");
        }
        Cipher cipher = Cipher.getInstance(CIPHER);
        cipher.init(Cipher.DECRYPT_MODE, key(), new GCMParameterSpec(TAG_BITS, kept, 1, IV_LENGTH));
        cipher.updateAAD(name);
        return cipher.doFinal(kept, 1 + IV_LENGTH, kept.length - 1 - IV_LENGTH);
    }

    /** Removes the secret kept as {@code key} of {@code service}; whether there was one. */
    public static boolean secretRemove(Context context, String service, String key)
            throws GeneralSecurityException, IOException {
        File file = secretFile(context, name(service, key));
        if (!file.exists()) {
            return false;
        }
        if (!file.delete()) {
            throw new IOException("cannot remove " + file);
        }
        return true;
    }
}
