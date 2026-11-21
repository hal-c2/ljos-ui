package dev.ljos;

import android.os.Bundle;
import androidx.activity.OnBackPressedCallback;
import com.google.androidgamesdk.GameActivity;

/**
 * The activity a Ljós application runs in. The application itself lives on
 * a thread of its own for the whole process; this activity only lends it a
 * screen, and hands it Back.
 */
public class LjosActivity extends GameActivity {
    /** Back, to the application: it decides what Back does. */
    private static native void nativeBack();

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        // With targetSdk 36 the system's Back (a gesture or the key) only
        // reaches the activity's back dispatcher, never onBackPressed nor
        // the native side as a key. Left to the activity, Back finishes it.
        getOnBackPressedDispatcher().addCallback(this, new OnBackPressedCallback(true) {
            @Override
            public void handleOnBackPressed() {
                nativeBack();
            }
        });
    }
}
