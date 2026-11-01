# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-reflectionprobe.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-custommaterial.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Members a scene can set that change nothing drawn
  These members are part of the types' interfaces and corpus scenes set
  them; here they are kept and read back, and docs/quick3d.md says that
  drawing does not follow them.

  Rule: They hold what they are given

    Scenario: A model's
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Model {
          objectName: "m"; receivesReflections: true; castsReflections: false; levelOfDetailBias: 2
        }
        """
      Then the property "receivesReflections" of "m" is true
      And the property "castsReflections" of "m" is false
      And the property "levelOfDetailBias" of "m" is 2

    Scenario: A reflection probe's
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        ReflectionProbe {
          objectName: "r"; quality: ReflectionProbe.High; timeSlicing: ReflectionProbe.AllFacesAtOnce
          refreshMode: ReflectionProbe.FirstFrame; parallaxCorrection: true
        }
        """
      Then the property "parallaxCorrection" of "r" is true

    Scenario: An environment's
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        SceneEnvironment {
          objectName: "e"; temporalAAEnabled: true; temporalAAStrength: 0.5; depthPrePassEnabled: true
        }
        """
      Then the property "temporalAAEnabled" of "e" is true
      And the property "temporalAAStrength" of "e" is 0.5
      And the property "depthPrePassEnabled" of "e" is true
