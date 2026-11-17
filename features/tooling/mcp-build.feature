# Sources:
#   docs/ARCHITECTURE.md section 19.3 (the authoring tools, `ljos mcp`)
#   https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#stdio
#   https://git-scm.com/docs/git-worktree
#   https://git-scm.com/docs/git-rev-parse (--git-common-dir)
#   https://doc.rust-lang.org/cargo/reference/environment-variables.html (CARGO_TARGET_DIR)
#   https://doc.rust-lang.org/cargo/reference/config.html (--config)
# `mise-tasks/mcp` is Ljós's own launcher: Qt has nothing to compare it with.
# The project is a checkout of its own, made a git repository with a linked worktree "wt" by the
# step that says so. Nothing is compiled: "bin/cargo" stands in for cargo and "bin/mise" for mise
# (first on the PATH the author's commands see), and each notes in "built.log" what it was asked
# to do. The stand-in cargo puts "server.sh" where the server would be built, and that answers as
# the server would. "build-wrapper.sh" stands in for a machine's build wrapper: it notes that the
# build went through it, says something on its output as a real one may, and runs the build.

@ljos-only @headless
Feature: Starting the authoring server from a checkout of Ljós
  `mise-tasks/mcp` starts `ljos mcp` on a checkout of Ljós: the repository's
  `.mcp.json` runs it for an editor or an agent, and so does `mise run mcp`.
  It builds the server first, then starts it. Every worktree of one
  repository builds into the target directory of the main checkout, so a new
  worktree does not make a build of its own; `CARGO_TARGET_DIR` overrides
  that. The build goes through the machine's build wrapper when
  `LJOS_BUILD_WRAPPER` names one, and through cargo alone when nothing does.
  `LJOS_BIN` names a server built already, which is started without a build.
  The server speaks JSON-RPC on its output, so nothing else is written there:
  the launcher says once, on its errors, which way it built, and the build's
  own output goes there too.

  Background:
    Given the file "Cargo.toml":
      """toml
      [workspace]
      """
    And the file "bin/cargo":
      """sh
      #!/bin/sh
      root=$(cd "$(dirname "$0")/.." && pwd -P)
      here=$(pwd -P)
      echo "cargo $* in .${here#"$root"} into .${CARGO_TARGET_DIR#"$root"}" | sed "s|$root|.|g" >> "$root/built.log"
      echo "   Compiling ljos_cli"
      echo "    Finished" >&2
      mkdir -p "$CARGO_TARGET_DIR/debug"
      cp "$root/server.sh" "$CARGO_TARGET_DIR/debug/ljos"
      """
    And the file "bin/mise":
      """sh
      #!/bin/sh
      root=$(cd "$(dirname "$0")/.." && pwd -P)
      echo "mise $1" >> "$root/built.log"
      shift
      [ "$1" = -- ] && shift
      exec "$@"
      """
    And the file "server.sh":
      """sh
      #!/bin/sh
      echo '{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"ljos"}}}'
      """
    And the file "build-wrapper.sh":
      """sh
      #!/bin/sh
      echo "through the build wrapper" >> "$(dirname "$0")/built.log"
      echo "build wrapper: a slot is free"
      exec "$@"
      """
    And the author's environment has "PATH" set to "{project}/bin:{PATH}"
    And the author's environment has no "CARGO_TARGET_DIR"
    And the author's environment has no "LJOS_BUILD_WRAPPER"
    And the author's environment has no "LJOS_BIN"

  Scenario: A linked worktree builds into the main checkout's target directory
    Given the project is a git checkout with the linked worktree "wt"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos in ./wt into ./target
      """
    And the project has no "wt/target"

  Scenario: The main checkout builds into its own target directory
    Given the project is a git checkout with the linked worktree "wt"
    When the author runs "mise-tasks/mcp"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos in . into ./target
      """

  Scenario: A checkout that is no git repository builds into its own target directory
    When the author runs "mise-tasks/mcp"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos in . into ./target
      """

  Scenario: A main checkout's target directory that is a link is followed to where it leads
    Given the file "elsewhere/README":
      """
      The builds of this checkout, on a drive with room for them.
      """
    And the project is a git checkout with the linked worktree "wt"
    And the project's directory "target" is a link to "elsewhere"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos in ./wt into ./elsewhere
      """
    And the project has no "wt/target"

  Scenario: CARGO_TARGET_DIR is where the build goes, whatever checkout it starts in
    Given the project is a git checkout with the linked worktree "wt"
    And the author's environment has "CARGO_TARGET_DIR" set to "{project}/shared"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos in ./wt into ./shared
      """

  Scenario: A linked worktree builds with the cargo settings the main checkout keeps outside git
    Given the project is a git checkout with the linked worktree "wt"
    And the file ".cargo/config.toml":
      """toml
      [build]
      jobs = 2
      """
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos --config ./.cargo/config.toml in ./wt into ./target
      """

  Scenario: The build goes through the build wrapper LJOS_BUILD_WRAPPER names
    Given the project is a git checkout with the linked worktree "wt"
    And the author's environment has "LJOS_BUILD_WRAPPER" set to "{project}/build-wrapper.sh"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      through the build wrapper
      cargo build --quiet -p ljos_cli --bin ljos in ./wt into ./target
      """
    And the command's output mentions "build-wrapper.sh" once

  Scenario: With no build wrapper named, the build is cargo's alone, and the launcher says so once
    Given the project is a git checkout with the linked worktree "wt"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the command's output mentions "with cargo alone" once
    And the command's output mentions "LJOS_BUILD_WRAPPER" once

  Scenario: A build wrapper that is not there stops the launcher before anything is built
    Given the project is a git checkout with the linked worktree "wt"
    And the author's environment has "LJOS_BUILD_WRAPPER" set to "{project}/no-such-wrapper"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 1
    And the command's output mentions "no-such-wrapper"
    And the project has no "built.log"

  Scenario: A checkout that pins its tools in mise.toml builds with them
    Given the file "mise.toml":
      """toml
      [tools]
      rust = "1.98.1"
      """
    And the project is a git checkout with the linked worktree "wt"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      mise exec
      cargo build --quiet -p ljos_cli --bin ljos in ./wt into ./target
      """

  Scenario: The launcher's output is the server's alone
    Given the project is a git checkout with the linked worktree "wt"
    And the author's environment has "LJOS_BUILD_WRAPPER" set to "{project}/build-wrapper.sh"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command's standard output is:
      """json
      {"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"ljos"}}}
      """

  Scenario: A server LJOS_BIN names is started without a build
    Given the project is a git checkout with the linked worktree "wt"
    And the author's environment has "LJOS_BIN" set to "{project}/server.sh"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the command's standard output is:
      """json
      {"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"ljos"}}}
      """
    And the project has no "built.log"
    And the project has no "target"
