# Qt's compiled QML for the spike's cases: World.qml, and WorldTyped.qml
# (the same with the annotations qmlcachegen asks for), compiled through
# qt_add_qml_module as an application ships them, and timed from C++ the
# way src/bin/aot_bench.rs times Ljós. See docs/aot-coverage.md, "Against
# Qt's compiled QML".
#
#   cmake -S examples/aot_spike/qt-aot -B <build> -G Ninja -DCMAKE_BUILD_TYPE=Release
#   cmake --build <build>
#   cmake --build <build> --target all_aotstats   # what qmlcachegen compiled
cmake_minimum_required(VERSION 3.21)
project(ljos_aot_spike_qt LANGUAGES CXX)

find_package(Qt6 6.8 REQUIRED COMPONENTS Gui Qml Quick)
qt_standard_project_setup(REQUIRES 6.8)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(QT_QML_GENERATE_AOTSTATS ON)

qt_add_executable(qt_aot_bench main.cpp)

# The file Ljós compiles is the one Qt compiles: no copy that can drift.
set_source_files_properties(../qml/World.qml PROPERTIES QT_RESOURCE_ALIAS World.qml)
qt_add_qml_module(qt_aot_bench
    URI AotCases
    VERSION 1.0
    QML_FILES ../qml/World.qml WorldTyped.qml
)

target_link_libraries(qt_aot_bench PRIVATE Qt6::Gui Qt6::Qml Qt6::Quick)
