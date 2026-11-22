package android.view.inputmethod;

/**
 * A restart closes the served connection before it returns, as API 36 does
 * on the main thread.
 */
public final class InputMethodManager {
    public static boolean served;

    public static void restartInput(BaseInputConnection connection) {
        if (served) {
            connection.closeConnection();
        }
    }
}
