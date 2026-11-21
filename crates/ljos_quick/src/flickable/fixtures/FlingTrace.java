import android.content.Context;
import android.content.ContextWrapper;
import android.content.pm.ApplicationInfo;
import android.content.res.Resources;
import android.view.animation.AnimationUtils;
import android.widget.OverScroller;
import java.lang.reflect.Method;

/** Android's own fling, sampled at exact times: OverScroller with its animation clock locked. */
public final class FlingTrace {
    public static void main(String[] args) throws Exception {
        final Resources res = Resources.getSystem();
        Context context = new ContextWrapper(null) {
            @Override public Resources getResources() { return res; }
            @Override public ApplicationInfo getApplicationInfo() {
                ApplicationInfo info = new ApplicationInfo();
                info.targetSdkVersion = 36;
                return info;
            }
        };
        float density = res.getDisplayMetrics().density;
        Method lock = AnimationUtils.class.getMethod("lockAnimationClock", long.class);
        System.out.println("density " + density);
        long t0 = 1_000_000L;
        for (String arg : args) {
            float dp = Float.parseFloat(arg);
            int v = Math.round(dp * density);
            lock.invoke(null, t0);
            OverScroller s = new OverScroller(context);
            s.fling(0, 0, 0, v, 0, 0, -1_000_000, 1_000_000);
            System.out.println("fling " + dp + " dp/s = " + v + " px/s final " + s.getFinalY() + " px");
            for (int ms = 0; ; ms += 16) {
                lock.invoke(null, t0 + ms);
                boolean more = s.computeScrollOffset();
                System.out.println("  " + ms + " " + s.getCurrY() + " " + s.getCurrVelocity());
                if (!more) break;
            }
        }
    }
}
