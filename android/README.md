# The Android project template

`ljos android bundle` and `ljos android run` copy this Gradle project into
the target directory (`<target>/android/<package>/`), fill in the
application's id, label and library, put the library cargo-ndk built for
each ABI into `app/src/main/jniLibs`, and run Gradle. Nothing here is built
in place.

| Placeholder | What `ljos android` writes |
|---|---|
| `@APPLICATION_ID@` | `--application-id`, by default `dev.ljos.<package>` |
| `@LABEL@` | `--label`, by default the package's name |
| `@LIB_NAME@` | the package's library (`lib<name>.so`) |

- GameActivity 4.4.0 (what android-activity 0.6.1 is written against),
  minSdk 26, targetSdk 36, arm64-v8a and x86_64.
- The manifest takes every configuration change, so a rotation, a fold or a
  new font scale resizes the running application instead of making the
  activity again (ARCHITECTURE.md §10.2).
- `dev.ljos.LjosActivity` hands Back to the application through the
  activity's back dispatcher (since API 33 the system's gesture never
  reaches the native side as a key).
- Gradle 8.13 with AGP 8.13 runs on JDK 21; `ljos android` passes the JDK in
  `JAVA_HOME` to Gradle. Release builds are signed with the debug key until
  the application gives its own (`--gradle-arg`).
