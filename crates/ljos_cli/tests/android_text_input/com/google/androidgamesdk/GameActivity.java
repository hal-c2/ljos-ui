package com.google.androidgamesdk;

import android.content.Context;
import android.view.View;
import android.view.inputmethod.EditorInfo;
import com.google.androidgamesdk.gametextinput.InputConnection;
import com.google.androidgamesdk.gametextinput.Listener;
import com.google.androidgamesdk.gametextinput.Settings;
import com.google.androidgamesdk.gametextinput.State;
import java.util.ArrayList;
import java.util.List;

/** The listener, keeping what the native side was told. */
public final class GameActivity extends Context implements Listener {
    final List<String> told = new ArrayList<>();

    @Override
    public void stateChanged(State state, boolean dismissed) {
        told.add(state.toString());
    }

    public class InputEnabledSurfaceView extends View {
        InputConnection mInputConnection;

        public InputEnabledSurfaceView(GameActivity activity) {
            mInputConnection =
                    new InputConnection(activity, this, new Settings(new EditorInfo(), false));
        }
    }
}
