// The spike's cases in Qt, driven from native code as src/bin/aot_bench.rs
// drives them in Ljós: each step writes the case's inputs by property name,
// then triggers it (reads the binding, or emits the handler's signal by
// name), and the result is read back outside the clock.
//
//   qt_aot_bench cases|steps|rss|snapshot <source>
//
// <source> is `module:World` or `module:WorldTyped`, the files
// qt_add_qml_module compiled, or a path to a .qml file, which is loaded
// uncompiled: from the file, with the disk cache off, so it runs as
// bytecode through Qt's interpreter and JIT (QV4_FORCE_INTERPRETER=1 in
// the environment keeps it in the interpreter).
//
// `cases` times the trigger alone, `steps` the inputs and the trigger: Qt
// evaluates a binding when its inputs change, so for c1, c4 and c5 the work
// is in the inputs and only the step compares across engines.

#include <QAbstractItemModel>
#include <QFile>
#include <QGuiApplication>
#include <QMetaObject>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QVariant>

#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <memory>
#include <string>
#include <vector>

namespace {

constexpr int Samples = 2000;
constexpr int Warm = 200;
const char *const Cases[] = {"c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8"};

long status(const char *field)
{
    QFile f(QStringLiteral("/proc/self/status"));
    if (!f.open(QIODevice::ReadOnly))
        return -1;
    const QByteArray key(field);
    for (const QByteArray &line : f.readAll().split('\n')) {
        if (line.startsWith(key))
            return line.mid(key.size()).trimmed().split(' ').first().toLong();
    }
    return -1;
}

[[noreturn]] void fail(const QString &why)
{
    std::fprintf(stderr, "qt_aot_bench: %s\n", qPrintable(why));
    std::exit(1);
}

// One world: an engine and the root World created from the source.
struct World
{
    std::unique_ptr<QQmlEngine> engine;
    std::unique_ptr<QObject> root;

    explicit World(const QString &source)
        : engine(std::make_unique<QQmlEngine>())
    {
        QQmlComponent c(engine.get());
        if (source.startsWith(QLatin1String("module:")))
            c.loadFromModule("AotCases", source.mid(7));
        else
            c.loadUrl(QUrl::fromLocalFile(source));
        root.reset(c.create());
        if (!root)
            fail(c.errorString());
        // As in Ljós's harness, counting starts after creation.
        root->setProperty("syncs", 0);
    }

    // By name on every call, as the Rust harness finds `editor` each time.
    QObject *find(const char *name) const
    {
        auto *o = root->findChild<QObject *>(QLatin1String(name));
        if (!o)
            fail(QStringLiteral("no object %1").arg(QLatin1String(name)));
        return o;
    }

    void prepare(const std::string &c, int k) const
    {
        QObject *r = root.get();
        static const char *const statuses[] = {"todo", "doing", "blocked", "done", "dropped", "archived"};
        if (c == "c1") {
            const double totals[] = {10, 0, 7};
            r->setProperty("total", totals[k % 3]);
            r->setProperty("done", double(k % 11));
        } else if (c == "c2") {
            r->setProperty("stopMode", k % 2 == 0);
        } else if (c == "c4") {
            r->setProperty("status", QString::fromLatin1(statuses[k % 6]));
        } else if (c == "c5") {
            static const QVariantList changes[] = {
                {false, true, false}, {}, {true}, {false}};
            r->setProperty("changes", changes[k % 4]);
            r->setProperty("health", k % 3 == 0);
            r->setProperty("runStatus", QString::fromLatin1(k % 5 == 4 ? "doing" : "proposed"));
        } else if (c == "c7") {
            r->setProperty("depth", k % 3);
            find("field")->setProperty("text", QString::fromLatin1(k % 2 == 0 ? "" : "x"));
        }
    }

    void trigger(const std::string &c, int k) const
    {
        QObject *r = root.get();
        bool ok = true;
        if (c == "c1") {
            sink(r->property("progress"));
        } else if (c == "c2") {
            ok = QMetaObject::invokeMethod(r, "clicked");
        } else if (c == "c3") {
            ok = QMetaObject::invokeMethod(find("editor"), "submitted",
                                           Q_ARG(QString, QString::fromLatin1(k % 2 == 0 ? "foreground" : "background")));
        } else if (c == "c4") {
            sink(r->property("statusText"));
        } else if (c == "c5") {
            sink(r->property("runText"));
        } else if (c == "c6") {
            static const char *const values[] = {"a", "b", "c", "a", "b"};
            ok = QMetaObject::invokeMethod(r, "picked", Q_ARG(QString, QString::fromLatin1(values[k % 5])));
        } else if (c == "c7") {
            static const int keys[4][2] = {{0x31, 0x04000000}, {0x01000015, 0}, {0x01000003, 0}, {0x41, 0}};
            const int key = k % 4 == 0 ? keys[0][0] + k % 9 : keys[k % 4][0];
            ok = QMetaObject::invokeMethod(r, "keyPressed", Q_ARG(int, key), Q_ARG(int, keys[k % 4][1]));
        } else if (c == "c8") {
            r->setProperty("items", items(k));
        } else {
            fail(QStringLiteral("no case %1").arg(QString::fromStdString(c)));
        }
        if (!ok)
            fail(QStringLiteral("%1 did not trigger").arg(QString::fromStdString(c)));
    }

    // What the handlers wrote, read after the clock stops.
    void readBack() const
    {
        for (const char *p : {"lastMode", "selected", "ran", "cursor", "accepted", "syncs"})
            sink(root->property(p));
    }

    static QVariantList items(int k)
    {
        switch (k % 5) {
        case 0: return {1.0, 2.0, 3.0};
        case 1: return {3.0, 1.0};
        case 2: return {4.0, 1.0, 2.0, 5.0};
        case 3: return {};
        default: return {5.0, 4.0, 3.0, 2.0, 1.0, 6.0};
        }
    }

    static void sink(const QVariant &v)
    {
        static volatile int s;
        s = s + v.isValid();
    }
};

long percentile(const std::vector<long> &sorted, double p)
{
    return sorted[size_t(double(sorted.size() - 1) * p + 0.5)];
}

void cases(const QString &source, bool step)
{
    using Clock = std::chrono::steady_clock;
    std::printf("case\tmedian_ns\tp99_ns\n");
    for (const char *name : Cases) {
        const std::string c(name);
        World w(source);
        for (int k = 0; k < Warm; ++k) {
            w.prepare(c, k);
            w.trigger(c, k);
        }
        std::vector<long> ns;
        ns.reserve(Samples);
        for (int k = Warm; k < Warm + Samples; ++k) {
            if (!step)
                w.prepare(c, k);
            const auto t = Clock::now();
            if (step)
                w.prepare(c, k);
            w.trigger(c, k);
            ns.push_back(long(std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - t).count()));
            w.readBack();
        }
        std::sort(ns.begin(), ns.end());
        std::printf("%s\t%ld\t%ld\n", name, percentile(ns, 0.5), percentile(ns, 0.99));
    }
}

void drive(const World &w)
{
    for (int k = 0; k < Samples; ++k) {
        for (const char *c : Cases) {
            w.prepare(c, k);
            w.trigger(c, k);
        }
    }
}

QString show(const QVariant &v)
{
    if (v.typeId() == QMetaType::QString)
        return QLatin1Char('"') + v.toString() + QLatin1Char('"');
    if (v.canConvert<QVariantList>() && v.typeId() != QMetaType::QString) {
        QStringList parts;
        for (const QVariant &e : v.toList())
            parts << show(e);
        return QLatin1Char('[') + parts.join(QLatin1Char(',')) + QLatin1Char(']');
    }
    return v.toString();
}

// The end state, in aot_bench snapshot's format, to check that both
// drivers did the same work.
void snapshot(const QString &source)
{
    World w(source);
    drive(w);
    for (const char *p : {"progress", "statusText", "runText", "lastMode", "selected", "ran", "cursor", "depth",
                          "accepted", "syncs"})
        std::printf("%s=%s\n", p, qPrintable(show(w.root->property(p))));
    auto *cards = qobject_cast<QAbstractItemModel *>(w.find("cards"));
    if (!cards)
        fail(QStringLiteral("cards is not a model"));
    const int role = cards->roleNames().key("toastId", -1);
    QStringList ids;
    for (int i = 0; i < cards->rowCount(); ++i)
        ids << QString::number(cards->data(cards->index(i, 0), role).toDouble(), 'f', 1);
    std::printf("cards=[%s]\n", qPrintable(ids.join(QLatin1String(", "))));
}

} // namespace

int main(int argc, char **argv)
{
    const long start = status("VmRSS:");
    if (argc != 3)
        fail(QStringLiteral("usage: qt_aot_bench cases|steps|rss|snapshot module:World|module:WorldTyped|<file.qml>"));
    const QString kind = QString::fromLocal8Bit(argv[1]);
    const QString source = QString::fromLocal8Bit(argv[2]);
    if (!source.startsWith(QLatin1String("module:")))
        qputenv("QML_DISABLE_DISK_CACHE", "1");
    QGuiApplication app(argc, argv);

    if (kind == QLatin1String("cases") || kind == QLatin1String("steps")) {
        cases(source, kind == QLatin1String("steps"));
    } else if (kind == QLatin1String("rss")) {
        World w(source);
        const long loaded = status("VmRSS:");
        drive(w);
        std::printf("mode\tstart_kb\tloaded_kb\tafter_kb\thwm_kb\n");
        std::printf("%s\t%ld\t%ld\t%ld\t%ld\n", qPrintable(source), start, loaded, status("VmRSS:"),
                    status("VmHWM:"));
    } else if (kind == QLatin1String("snapshot")) {
        snapshot(source);
    } else {
        fail(QStringLiteral("no kind %1").arg(kind));
    }
    return 0;
}
