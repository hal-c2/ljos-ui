# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#active-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#item-prop
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#destruction-signal
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (loaders), seeds 101 and 202: buckets value:onCompleted:text,
# value:onChanged+onCompleted:text and value:log:empty-ljos (see
# crates/ljos_fuzz/known/loaders.txt); seed 303, value:onChanged+onCompleted:empty-ljos, and after the
# first fixes value:log:text (a component swapped) and value:onCompleted:text (a bound active).

@qt-parity
Feature: The order in which a Loader tells of its object
  A Loader turned off destroys its object and tells that its item and then its status changed before it
  tells that it is no longer active. One turned on with nothing to load still tells that its
  progress and its item changed, and one turned off with nothing loaded tells only that it is no
  longer active. This holds for an `active` that is bound as for one that is written. One given
  something else to load tells of its new item with no empty item between, and one that cannot
  load it tells of its error before its empty item.

  Rule: A Loader turned on or off follows before it tells that it is active or not

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string log: ""
          property bool on: true
          Component { id: c
            Item { objectName: "m"
              Component.onCompleted: root.log += "+m;"
              Component.onDestruction: root.log += "-m;" } }
          Loader { id: l; objectName: "l"; sourceComponent: c
            onItemChanged: root.log += "item=" + (item ? item.objectName : "null") + ";"
            onStatusChanged: root.log += "status=" + status + ";"
            onActiveChanged: root.log += "active=" + active + ";" }
          Loader { id: b; objectName: "b"; sourceComponent: c; active: root.on
            onItemChanged: root.log += "b.item=" + (item ? item.objectName : "null") + ";"
            onActiveChanged: root.log += "b.active=" + active + ";" }
          function off() { log = ""; l.active = false }
          function bound() { log = "off:"; on = false; log += "|on:"; on = true }
        }
        """

    Scenario: A Loader turned off tells of its item and status before it is inactive
      When the method "off" of "root" is called
      Then the property "log" of "root" is "-m;item=null;status=0;active=false;"

    Scenario: A Loader whose active is bound loads or unloads before it tells that it changed
      When the method "bound" of "root" is called
      Then the property "log" of "root" is "off:-m;b.item=null;b.active=false;|on:+m;b.item=m;b.active=true;"

  Rule: A Loader with nothing to load tells that its progress and item changed when turned on

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string log: ""
          component Told: Loader { active: false
            onItemChanged: root.log += "e.item=" + (item ? item.objectName : "null") + ";"
            onProgressChanged: root.log += "e.progress=" + progress + ";"
            onStatusChanged: root.log += "e.status=" + status + ";"
            onActiveChanged: root.log += "e.active=" + active + ";" }
          Told { id: e }
          Told { id: a; asynchronous: true }
          function toggle(it) {
            log = "on:"; it.active = true;
            log += "|off:"; it.active = false;
            log += "|on:"; it.active = true;
            log += "|again:"; it.active = true;
            log += "|null:"; it.sourceComponent = null;
            log += "|empty:"; it.source = ""
          }
          function onEmpty() { toggle(e) }
          function onEmptyAsync() { toggle(a) }
        }
        """

    Scenario Outline: A Loader with nothing to load tells that its progress and item changed when turned on, and only then
      When the method "<turn>" of "root" is called
      Then the property "log" of "root" is "on:e.progress=0;e.item=null;e.active=true;|off:e.active=false;|on:e.progress=0;e.item=null;e.active=true;|again:|null:|empty:"

      Examples:
        | turn         |
        | onEmpty      |
        | onEmptyAsync |

  Rule: A Loader given something else to load tells of the new item with no empty one between

    Background:
      Given the file "Other.qml":
        """qml
        import QtQuick
        Item { objectName: "o" }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string log: ""
          Component { id: d; QtObject { objectName: "n" } }
          Component { id: c
            Item { objectName: "m"
              Component.onCompleted: root.log += "+m;"
              Component.onDestruction: root.log += "-m;" } }
          Loader { id: l; objectName: "l"; sourceComponent: c
            onLoaded: root.log += "loaded;"
            onItemChanged: root.log += "item=" + (item ? item.objectName : "null") + ";"
            onStatusChanged: root.log += "status=" + status + ";" }
          function swap() {
            log = "component:"; l.sourceComponent = d;
            log += "|file:"; l.source = "Other.qml";
            log += "|back:"; l.sourceComponent = c;
            log += "|missing:"; l.source = "Missing.qml"
          }
        }
        """

    Scenario: A Loader given something else to load tells of the new item, with no empty one between, or of its error before its empty item
      When the method "swap" of "root" is called
      Then the property "log" of "root" is "component:-m;item=n;loaded;|file:item=o;loaded;|back:+m;item=m;loaded;|missing:-m;status=3;item=null;"
