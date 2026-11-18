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
# to do. The stand-in cargo puts "server.sh" where the server would be built (under the build
# target CARGO_BUILD_TARGET names, if any), says where in a JSON message as cargo does, and the copy
# answers as the server would. "build-wrapper.sh" stands in for a machine's build wrapper: it notes that the
# build went through it, says something on its output as a real one may, and runs the build.

@ljos-only @headless
Feature: Starting the authoring server from a checkout of Ljós
  `mise-tasks/mcp` starts `ljos mcp` on a checkout of Ljós: the repository's
  `.mcp.json` runs it for an editor or an agent, and so does `mise run mcp`.
  It builds the server first, then starts it. A linked worktree whose own
  cargo settings name no target builds into the target directory of the
  main checkout (the `build.target-dir` its cargo settings name, else its
  `target/`), so a new worktree does not make a build of its own; a
  `build.target-dir` in the worktree's own settings is where it builds, and
  `CARGO_TARGET_DIR`, then `CARGO_BUILD_TARGET_DIR`, override them all.
  The build goes through the machine's build wrapper when
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
      printf '%s\n' "cargo $* in .${here#"$root"}" | sed "s|$root|.|g" >> "$root/built.log"
      [ "$CARGO_TARGET_DIR" = "$(printf '%s\n' "$*" | sed 's/.*--target-dir \([^ ]*\).*/\1/')" ] ||
        echo "CARGO_TARGET_DIR is not the --target-dir" >> "$root/built.log"
      out="$CARGO_TARGET_DIR${CARGO_BUILD_TARGET:+/$CARGO_BUILD_TARGET}/debug"
      mkdir -p "$out"
      cp "$root/server.sh" "$out/ljos"
      echo "   Compiling ljos_cli"
      exe=$(printf '%s' "$out/ljos" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')
      printf '%s\n' "{\"reason\":\"compiler-artifact\",\"executable\":\"$exe\",\"fresh\":false}"
      echo '{"reason":"build-finished","success":true}'
      echo "    Finished" >&2
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
    And the author's environment has no "CARGO_BUILD_TARGET_DIR"
    And the author's environment has "CARGO_HOME" set to "{project}/cargo-home"
    And the author's environment has no "LJOS_BUILD_WRAPPER"
    And the author's environment has no "LJOS_BIN"
    And the author's environment has no "CARGO_BUILD_TARGET"

  Scenario: A linked worktree builds into the main checkout's target directory
    Given the project is a git checkout with the linked worktree "wt"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos --message-format=json-render-diagnostics --target-dir ./target in ./wt
      """
    And the project has no "wt/target"

  Scenario: The main checkout builds into its own target directory
    Given the project is a git checkout with the linked worktree "wt"
    When the author runs "mise-tasks/mcp"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos --message-format=json-render-diagnostics --target-dir ./target in .
      """

  Scenario: A checkout that is no git repository builds into its own target directory
    When the author runs "mise-tasks/mcp"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos --message-format=json-render-diagnostics --target-dir ./target in .
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
      cargo build --quiet -p ljos_cli --bin ljos --message-format=json-render-diagnostics --target-dir ./elsewhere in ./wt
      """
    And the project has no "wt/target"

  Scenario: CARGO_TARGET_DIR is where the build goes, whatever checkout it starts in
    Given the project is a git checkout with the linked worktree "wt"
    And the author's environment has "CARGO_TARGET_DIR" set to "{project}/shared"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos --message-format=json-render-diagnostics --target-dir ./shared in ./wt
      """

  Scenario: A target directory whose name JSON escapes is where the server is started from
    Given the project is a git checkout with the linked worktree "wt"
    And the author's environment has "CARGO_TARGET_DIR" set to "{project}/shared\builds"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos --message-format=json-render-diagnostics --target-dir ./shared\builds in ./wt
      """
    And the command's standard output is:
      """json
      {"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"ljos"}}}
      """

  # This cargo builds into a directory whose name holds a tab, which JSON writes as \t. Beside it
  # is one named with a backslash and a "t", whose stale program answers otherwise.
  Scenario: A build directory whose name holds a tab is where the server is started from
    Given the project is a git checkout with the linked worktree "wt"
    And the file "bin/cargo":
      """sh
      #!/bin/sh
      root=$(cd "$(dirname "$0")/.." && pwd -P)
      out="$CARGO_TARGET_DIR/$(printf 'tab\tbuilds')/debug"
      stale="$CARGO_TARGET_DIR/tab\\tbuilds/debug"
      mkdir -p "$out" "$stale"
      cp "$root/server.sh" "$out/ljos"
      printf '#!/bin/sh\necho stale\n' > "$stale/ljos"
      chmod +x "$stale/ljos"
      dir=$(printf '%s' "$CARGO_TARGET_DIR" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')
      printf '%s\n' "{\"reason\":\"compiler-artifact\",\"executable\":\"$dir/tab\\tbuilds/debug/ljos\",\"fresh\":false}"
      """
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the command's standard output is:
      """json
      {"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"ljos"}}}
      """

  Scenario: A linked worktree builds into the target directory its main checkout's cargo settings name
    Given the project is a git checkout with the linked worktree "wt"
    And the file ".cargo/config.toml":
      """toml
      [build]
      jobs = 2
      target-dir = "elsewhere"
      """
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      cargo build --quiet -p ljos_cli --bin ljos --message-format=json-render-diagnostics --target-dir ./elsewhere in ./wt
      """
    And the project has no "wt/target"

  Scenario: The build goes through the build wrapper LJOS_BUILD_WRAPPER names
    Given the project is a git checkout with the linked worktree "wt"
    And the author's environment has "LJOS_BUILD_WRAPPER" set to "{project}/build-wrapper.sh"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the file "built.log" is:
      """
      through the build wrapper
      cargo build --quiet -p ljos_cli --bin ljos --message-format=json-render-diagnostics --target-dir ./target in ./wt
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
      cargo build --quiet -p ljos_cli --bin ljos --message-format=json-render-diagnostics --target-dir ./target in ./wt
      """

  Scenario: The launcher's output is the server's alone
    Given the project is a git checkout with the linked worktree "wt"
    And the author's environment has "LJOS_BUILD_WRAPPER" set to "{project}/build-wrapper.sh"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command's standard output is:
      """json
      {"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"ljos"}}}
      """

  Scenario: With a build target set, the server started is the one the build made under it
    Given the project is a git checkout with the linked worktree "wt"
    And the file "target/debug/ljos":
      """sh
      #!/bin/sh
      echo '{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"an older build"}}}'
      """
    And the author's environment has "CARGO_BUILD_TARGET" set to "x86_64-unknown-linux-gnu"
    When the author runs "mise-tasks/mcp" in "wt"
    Then the command exits with status 0
    And the command's standard output is:
      """json
      {"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"ljos"}}}
      """

  # .mcp.json runs `bash mise-tasks/mcp .` without mise. The editor or agent that runs it may have
  # been started by a mise task of another project, and then hands down all of mise's variables:
  # where that task was started, its name and its file. "project.sh" stands in for the server and
  # says which directory it was given to author.
  Scenario: Run outside mise, "." is the checkout it runs in
    Given the file "project.sh":
      """sh
      #!/bin/sh
      root=$(cd "$(dirname "$0")" && pwd -P)
      echo "authoring .${3#"$root"}"
      """
    And the file "elsewhere/README":
      """
      Where a mise task of another project was started.
      """
    And the project is a git checkout with the linked worktree "wt"
    And the author's environment has "LJOS_BIN" set to "{project}/project.sh"
    And the author's environment has "MISE_ORIGINAL_CWD" set to "{project}/elsewhere"
    And the author's environment has no "MISE_TASK_NAME"
    And the author's environment has no "MISE_TASK_FILE"
    When the author runs "mise-tasks/mcp ." in "wt"
    Then the command exits with status 0
    And the command's standard output is:
      """
      authoring ./wt
      """

  Scenario: Run outside mise by a program another project's mise task started, "." is still the checkout it runs in
    Given the file "project.sh":
      """sh
      #!/bin/sh
      root=$(cd "$(dirname "$0")" && pwd -P)
      echo "authoring .${3#"$root"}"
      """
    And the file "elsewhere/mise-tasks/agents":
      """sh
      #!/bin/sh
      echo "Another project's task, which started the agent."
      """
    And the project is a git checkout with the linked worktree "wt"
    And the author's environment has "LJOS_BIN" set to "{project}/project.sh"
    And the author's environment has "MISE_ORIGINAL_CWD" set to "{project}/elsewhere"
    And the author's environment has "MISE_TASK_NAME" set to "agents"
    And the author's environment has "MISE_TASK_FILE" set to "{project}/elsewhere/mise-tasks/agents"
    When the author runs "mise-tasks/mcp ." in "wt"
    Then the command exits with status 0
    And the command's standard output is:
      """
      authoring ./wt
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
