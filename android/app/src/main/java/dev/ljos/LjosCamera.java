package dev.ljos;

import android.content.Context;
import android.graphics.ImageFormat;
import android.hardware.camera2.CameraAccessException;
import android.hardware.camera2.CameraCaptureSession;
import android.hardware.camera2.CameraCharacteristics;
import android.hardware.camera2.CameraDevice;
import android.hardware.camera2.CameraManager;
import android.hardware.camera2.CaptureRequest;
import android.hardware.camera2.params.StreamConfigurationMap;
import android.media.Image;
import android.media.ImageReader;
import android.os.Handler;
import android.os.HandlerThread;
import android.util.Log;
import android.util.Size;
import android.view.Surface;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

/**
 * The system's cameras, for the application's native side (ljos_media's
 * AndroidCameras), through Camera2: each capture streams YUV_420_888
 * pictures into an ImageReader on a thread of its own, and hands each one's
 * planes to the native side as they are, with their strides, before the
 * reader takes the picture back.
 *
 * <p>A capture is named by the native side's handle; what it says of the
 * capture (a picture, its loss) it says by that handle, and nothing after
 * {@link #close}.
 */
public final class LjosCamera {
    private static final String TAG = "LjosCamera";

    /** The size asked for, the nearest the camera has; the same as V4L2's. */
    private static final int WIDTH = 640;
    private static final int HEIGHT = 480;

    /** The captures open, by handle. */
    private static final Map<Long, Capture> open = Collections.synchronizedMap(new HashMap<>());

    private LjosCamera() {}

    private static native void nativeFrame(
            long handle,
            int width,
            int height,
            java.nio.ByteBuffer y,
            int yRow,
            java.nio.ByteBuffer u,
            java.nio.ByteBuffer v,
            int uvRow,
            int uvPixel);

    private static native void nativeLost(long handle, String why);

    /**
     * The system's cameras, a line each: the id, a tab, and where it faces
     * ({@code back}, {@code front} or {@code external}).
     */
    public static String list(Context context) throws CameraAccessException {
        CameraManager manager = context.getSystemService(CameraManager.class);
        StringBuilder out = new StringBuilder();
        for (String id : manager.getCameraIdList()) {
            Integer facing =
                    manager.getCameraCharacteristics(id).get(CameraCharacteristics.LENS_FACING);
            String where = "external";
            if (facing != null && facing == CameraCharacteristics.LENS_FACING_BACK) {
                where = "back";
            } else if (facing != null && facing == CameraCharacteristics.LENS_FACING_FRONT) {
                where = "front";
            }
            out.append(id).append('\t').append(where).append('\n');
        }
        return out.toString();
    }

    /**
     * Opens the camera {@code id} for the capture {@code handle}. The
     * camera opens on the capture's thread; a camera that does not open is
     * lost ({@code nativeLost}). Throws when the system refuses at once
     * (no permission, no such camera).
     */
    public static void open(Context context, String id, long handle)
            throws CameraAccessException {
        CameraManager manager = context.getSystemService(CameraManager.class);
        Size size = nearest(manager.getCameraCharacteristics(id));
        Capture capture = new Capture(handle, size);
        open.put(handle, capture);
        try {
            manager.openCamera(id, capture.stateCallback(), capture.handler);
        } catch (CameraAccessException | RuntimeException e) {
            open.remove(handle);
            capture.opening = false;
            capture.release();
            throw e;
        }
    }

    /** Closes the capture {@code handle}; nothing more is said of it. */
    public static void close(long handle) {
        Capture capture = open.remove(handle);
        if (capture != null) {
            capture.release();
        }
    }

    /** The YUV size nearest WIDTH by HEIGHT in area. */
    private static Size nearest(CameraCharacteristics characteristics) {
        StreamConfigurationMap map =
                characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP);
        Size best = new Size(WIDTH, HEIGHT);
        if (map == null) {
            return best;
        }
        Size[] sizes = map.getOutputSizes(ImageFormat.YUV_420_888);
        if (sizes == null || sizes.length == 0) {
            return best;
        }
        long want = (long) WIDTH * HEIGHT;
        long gap = Long.MAX_VALUE;
        for (Size s : sizes) {
            long g = Math.abs((long) s.getWidth() * s.getHeight() - want);
            if (g < gap) {
                gap = g;
                best = s;
            }
        }
        return best;
    }

    /** One capture: its thread, its reader, its device and session. */
    private static final class Capture {
        final long handle;
        final HandlerThread thread;
        final Handler handler;
        final ImageReader reader;
        CameraDevice device;
        CameraCaptureSession session;
        volatile boolean closed;
        /**
         * The open is outstanding: its callback has yet to come. Read and
         * written on the capture's thread only (or before it sees the
         * capture), as is {@link #released}.
         */
        boolean opening = true;
        /** {@link #release}'s cleanup has run on the capture's thread. */
        boolean released;

        Capture(long handle, Size size) {
            this.handle = handle;
            thread = new HandlerThread("ljos-camera");
            thread.start();
            handler = new Handler(thread.getLooper());
            reader =
                    ImageReader.newInstance(
                            size.getWidth(), size.getHeight(), ImageFormat.YUV_420_888, 2);
            reader.setOnImageAvailableListener(this::picture, handler);
        }

        CameraDevice.StateCallback stateCallback() {
            return new CameraDevice.StateCallback() {
                @Override
                public void onOpened(CameraDevice camera) {
                    if (closed) {
                        camera.close();
                        opened();
                        return;
                    }
                    opening = false;
                    device = camera;
                    stream();
                }

                @Override
                public void onDisconnected(CameraDevice camera) {
                    camera.close();
                    opened();
                    lost("The camera was disconnected");
                }

                @Override
                public void onError(CameraDevice camera, int error) {
                    camera.close();
                    opened();
                    lost(error == ERROR_CAMERA_IN_USE || error == ERROR_MAX_CAMERAS_IN_USE
                            ? "The camera is in use"
                            : "The camera failed (" + error + ")");
                }
            };
        }

        @SuppressWarnings("deprecation") // the list form is API 21; the newer is 28.
        void stream() {
            try {
                Surface surface = reader.getSurface();
                CaptureRequest.Builder request =
                        device.createCaptureRequest(CameraDevice.TEMPLATE_PREVIEW);
                request.addTarget(surface);
                device.createCaptureSession(
                        Collections.singletonList(surface),
                        new CameraCaptureSession.StateCallback() {
                            @Override
                            public void onConfigured(CameraCaptureSession s) {
                                if (closed) {
                                    s.close();
                                    return;
                                }
                                session = s;
                                try {
                                    s.setRepeatingRequest(request.build(), null, handler);
                                } catch (CameraAccessException | RuntimeException e) {
                                    lost(e.toString());
                                }
                            }

                            @Override
                            public void onConfigureFailed(CameraCaptureSession s) {
                                lost("The camera could not be configured");
                            }
                        },
                        handler);
            } catch (CameraAccessException | RuntimeException e) {
                lost(e.toString());
            }
        }

        void picture(ImageReader r) {
            Image image = r.acquireLatestImage();
            if (image == null) {
                return;
            }
            try {
                if (!closed) {
                    Image.Plane[] p = image.getPlanes();
                    nativeFrame(
                            handle,
                            image.getWidth(),
                            image.getHeight(),
                            p[0].getBuffer(),
                            p[0].getRowStride(),
                            p[1].getBuffer(),
                            p[2].getBuffer(),
                            p[1].getRowStride(),
                            p[1].getPixelStride());
                }
            } finally {
                image.close();
            }
        }

        void lost(String why) {
            if (closed) {
                return;
            }
            Log.w(TAG, "capture " + handle + ": " + why);
            nativeLost(handle, why);
        }

        /**
         * The open's callback came. A capture released while it was opening
         * ends only now: had its thread quit before, the callback would have
         * been dropped with the camera it opened, held until the process dies.
         */
        void opened() {
            opening = false;
            if (released) {
                thread.quitSafely();
            }
        }

        /**
         * Stops everything, on the capture's thread, and ends it once no
         * open is outstanding.
         */
        void release() {
            closed = true;
            handler.post(
                    () -> {
                        released = true;
                        if (session != null) {
                            session.close();
                        }
                        if (device != null) {
                            device.close();
                        }
                        reader.close();
                        if (!opening) {
                            thread.quitSafely();
                        }
                    });
        }
    }
}
