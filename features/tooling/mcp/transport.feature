# Sources:
#   docs/ARCHITECTURE.md sections 19.2, 19.5, 19.7 and 19.9
#   docs/research/10-mcp.md
#   https://modelcontextprotocol.io/specification/2025-11-25/basic/transports
#   https://modelcontextprotocol.io/specification/2025-11-25/basic/security_best_practices
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Who may talk to the inspector
  The inspector listens on this machine only and answers a client that
  shows the token of this start of the application. A web page open in a
  browser on the same machine is not such a client.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "save"
        width: 80; height: 30
        Accessible.role: Accessible.Button
        Accessible.name: "Save"
      }
      """
    And the inspector is served on the loopback address

  Scenario: A client with the token is answered
    When a client asks for a snapshot with the right token
    Then the client is answered
    And the client's answer contains "button \"Save\""

  Scenario Outline: A client without the token is turned away
    When a client asks for a snapshot with <token>
    Then the client is refused with status 401
    And the client's answer does not contain "Save"

    Examples:
      | token                       |
      | no token                    |
      | a wrong token               |
      | the token of an older start |

  Scenario: A web page is turned away even with the token
    When a client asks for a snapshot with the right token from the origin "https://example.com"
    Then the client is refused with status 403

  Scenario: A name that is not this machine's is turned away even with the token
    When a client asks for a snapshot with the right token for the host "example.com"
    Then the client is refused with status 403

  Scenario: The token is kept in a file only its owner can read
    Then the token file can be read by its owner alone
    And the token file names an address on this machine

  Scenario: The application rests again after it has answered
    When a client asks for a snapshot with the right token
    Then the client is answered
    And the application is left idle

  Rule: An inspector that is only connected costs nothing

    Scenario: Nothing is kept running for a client that asks nothing
      Then the application is left idle

    Scenario: The accessibility tree is given back after a request
      When a client asks for a snapshot with the right token
      Then the application is left idle
      And the accessibility tree is not being kept

  Rule: The inspector can listen on a socket in the file system

    Scenario: A socket is made that only its owner can use
      Given the inspector is served on a socket
      Then the socket can be used by its owner alone

    Scenario: A client on the socket is answered with the token
      Given the inspector is served on a socket
      When a client asks on the socket for a snapshot with the right token
      Then the client is answered
      And the client's answer contains "button \"Save\""

    Scenario: A client on the socket without the token is turned away
      Given the inspector is served on a socket
      When a client asks on the socket for a snapshot with no token
      Then the client is refused with status 401

    Scenario: The socket and the token file are gone when the inspector stops
      Given the inspector is served on a socket
      When the inspector is stopped
      Then the socket is gone
      And the token file is gone

    # A socket path has a length the system limits. Choosing another place
    # for it is the person's to do; the inspector never moves it silently.
    Scenario: A socket path that is too long is refused and says why
      When the inspector is asked to listen on a socket whose path is 300 characters long
      Then the inspector is refused with "300"
      And the inspector is refused with "at most"
      And the inspector is refused with "--mcp-socket"

    Scenario: A socket path that is as long as the system allows is served
      When the inspector is asked to listen on a socket whose path is as long as the system allows
      Then the inspector is served

  Rule: An address that is not this machine's needs two words

    Scenario: An address that is not this machine's is not served
      When the inspector is asked to listen on "0.0.0.0:0" without the word for a network
      Then the inspector is refused with "--mcp-insecure-lan"

    Scenario: An address on this machine is served on the port asked for
      When the inspector is asked to listen on "127.0.0.1:0" without the word for a network
      Then the inspector is served

    # Only what the two words allow is checked: a test never opens a port to
    # the network.
    Scenario: A network address is allowed with a warning when both words are given
      When the inspector is asked to listen on "0.0.0.0:0" with the word for a network
      Then the inspector is allowed to listen there
      And the warning mentions "anyone on the network"
