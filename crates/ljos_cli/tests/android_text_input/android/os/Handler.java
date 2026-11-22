package android.os;

import java.util.ArrayDeque;

/** The main thread's queue, run when the test says. */
public final class Handler {
    private static final ArrayDeque<Runnable> QUEUE = new ArrayDeque<>();

    public Handler(Looper looper) {}

    public boolean post(Runnable task) {
        QUEUE.addLast(task);
        return true;
    }

    public boolean postAtFrontOfQueue(Runnable task) {
        QUEUE.addFirst(task);
        return true;
    }

    public static void runQueued() {
        for (Runnable task; (task = QUEUE.pollFirst()) != null; ) {
            task.run();
        }
    }
}
