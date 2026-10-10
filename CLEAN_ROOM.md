# Clean-room policy

Ljós implements QML, Qt Quick and Qt Quick Controls from their **public behaviour**, never from their code.

## Never

- Read, copy, translate or paraphrase source code from Qt (any module, any licence) or Slint. That covers C++, QML style files, shaders, tests and build scripts. It applies to web views of their repositories and code snippets in search results too.
- Commit data generated from a Qt installation (`types.txt`, reference PNGs, `.qm` files) (ARCHITECTURE.md D1). Such data is generated locally and git-ignored.
- Read `.qm` files. Catalogues come from `.ts` XML only.

## Allowed

- Qt documentation (doc.qt.io), published specifications, blog posts and talks.
- Running Qt binaries as a black box: `qml6`, `lupdate`, `lrelease`, `qmltyperegistrar` outputs read through `.qmltypes`, screenshots. This is how the `@qt-parity` oracle works.
- Reading and depending on permissively licensed crates (egui, rquickjs, AccessKit, winit, wgpu, Parley, rmcp, …).
- Code from qml-solid, which belongs to this project's author (MIT).

## How it is enforced

- Every worker and contributor prompt carries this policy.
- Every `.feature` file starts with a `# Sources:` block naming documentation URLs or research reports.
- Qt Quick Controls styles are written from scratch from the documentation and screenshots.
- Reviewers reject changes whose provenance is unclear.
