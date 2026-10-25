# Sources:
#   docs/ARCHITECTURE.md section 19.7
#   https://url.spec.whatwg.org/
@ljos-only
Feature: Resource diagnostic URLs omit credentials
  Network resource warnings retain the host and path without exposing access credentials.

  Scenario Outline: Image and font failures are safe in warnings and inspector logs
    Given the QML:
      """qml
      import QtQuick
      Item {
        <type> { objectName: "resource"; source: "http://diagnostic-user:diagnostic-password@127.0.0.1:9/asset.<ext>?token=diagnostic-token#diagnostic-fragment" }
      }
      """
    And the inspector is connected with the capabilities "inspect"
    When 1 second passes
    Then the property "status" of "resource" is 3
    And the log has no warning matching "diagnostic-user|diagnostic-password|diagnostic-token|diagnostic-fragment"
    When the agent calls the tool "app_logs"
    Then the answer does not contain "diagnostic-password"
    And the answer does not contain "diagnostic-token"
    And the answer does not contain "diagnostic-fragment"

    Examples:
      | type       | ext |
      | Image      | png |
      | FontLoader | ttf |

  Scenario Outline: Server and decoder failures keep a useful public URL
    Given the web server serves a file "bad.png" that is not an image
    And the QML:
      """qml
      import QtQuick
      Item {
        <type> {
          objectName: "resource"
          source: "$SERVER/<file>".replace("://", "://diagnostic-user:diagnostic-password@") + "?token=diagnostic-token#diagnostic-fragment"
        }
      }
      """
    And the inspector is connected with the capabilities "inspect"
    When 1 second passes
    Then the property "status" of "resource" is 3
    And the log has a warning matching "http://127.*<file>"
    And the log has no warning matching "diagnostic-user|diagnostic-password|diagnostic-token|diagnostic-fragment"
    When the agent calls the tool "app_logs"
    Then the answer does not contain "diagnostic-password"
    And the answer does not contain "diagnostic-token"
    And the answer's text contains "<file>"

    Examples:
      | type       | file        |
      | Image      | missing.png |
      | Image      | bad.png     |
      | FontLoader | missing.png |
      | FontLoader | bad.png     |
