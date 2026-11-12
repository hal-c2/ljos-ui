# Sources:
#   https://fetch.spec.whatwg.org/ (fetch, Headers, Request, Response, the forbidden request headers)
#   https://dom.spec.whatwg.org/#interface-abortcontroller (AbortController, AbortSignal, timeout, any)
#   https://webidl.spec.whatwg.org/#idl-DOMException (AbortError 20, TimeoutError 23)
#   docs/DIALECT.md §4.19 (the subset Ljós has, what an error is, XMLHttpRequest as compat only)
# `fetch` as a QML script's way to the network is Ljós's own: Qt's QML has XMLHttpRequest
# (features/platform/xml-http-request.feature), and nothing here is compared with it.

@ljos-only
Feature: fetch: a script asks the network and awaits the reply
  A script of any file fetches with what a web page fetches with:
  `fetch`, `Request`, `Response`, `Headers`, and an `AbortController`
  whose signal stops a request. The request runs on a thread of the
  platform and its reply wakes the application, which has nothing to do
  while it waits. A status the server sends is a response, `ok` or not; a
  server that cannot be reached rejects with a TypeError, and a request
  stopped rejects with its signal's reason. XMLHttpRequest stays for files
  written against Qt's API, and `ljos migrate` rewrites its usual shape
  into `fetch`.

  Background:
    Given the web server serves the text file "act.json":
      """
      {"activity": "Read a book", "n": 3}
      """

  Rule: A reply is a Response, whatever its status

    Scenario: A script awaits a fetch and decodes the JSON it gives
      Given the QML:
        """qml
        import Ljos.Ui; import Ljos.Core
        Item {
          id: root; objectName: "root"
          property string got: ""
          Component.onCompleted: {
            const load = async (): Promise<void> => {
              const reply = await fetch("$SERVER/act.json")
              const given: unknown = await reply.json()
              let activity = "none"
              if (typeof given === "object" && given !== null && "activity" in given && typeof given.activity === "string") { activity = given.activity }
              root.got = `${reply.status} ${reply.ok} ${reply.statusText} ${reply.headers.get("content-type")} ${activity}`
            }
            load()
          }
        }
        """
      When 1 second passes
      Then the property "got" of "root" is "200 true OK application/json Read a book"
      And the application has nothing scheduled

    Scenario: A name the server does not have is a response that is not ok
      Given the QML:
        """qml
        import Ljos.Ui; import Ljos.Core
        Item {
          id: root; objectName: "root"
          property string got: ""
          Component.onCompleted: fetch("$SERVER/missing.json")
            .then(async (reply: Response) => { root.got = `${reply.status} ${reply.ok} ${reply.statusText} ${await reply.text()}` })
        }
        """
      When 1 second passes
      Then the property "got" of "root" is "404 false Not Found Not Found"

    Scenario: A script sends a body with its own headers, and those it may not set are not sent
      Given the QML:
        """qml
        import Ljos.Ui; import Ljos.Core
        Item {
          id: root; objectName: "root"
          property string got: ""
          Component.onCompleted: {
            const headers = new Headers({ "Content-Type": "application/json" })
            headers.append("X-One", "1")
            headers.append("x-one", "2")
            headers.set("Connection", "close")
            fetch("$SERVER/echo", { method: "POST", headers, body: JSON.stringify({ a: 1 }) })
              .then((reply: Response) => reply.text())
              .then((text: string) => { root.got = text })
          }
        }
        """
      When 1 second passes
      Then the property "got" of "root" is "POST\ncontent-type: application/json\nx-one: 1, 2\n\n{\"a\":1}"

    Scenario: A body of text is said to be text, and bytes are sent as they are
      Given the QML:
        """qml
        import Ljos.Ui; import Ljos.Core
        Item {
          id: root; objectName: "root"
          property string text: ""
          property string bytes: ""
          Component.onCompleted: {
            fetch(new Request("$SERVER/echo", { method: "PUT", body: "hi" }))
              .then((reply: Response) => reply.text()).then((t: string) => { root.text = t })
            fetch("$SERVER/echo", { method: "POST", body: new Uint8Array([104, 105, 33]) })
              .then((reply: Response) => reply.text()).then((t: string) => { root.bytes = t })
          }
        }
        """
      When 1 second passes
      Then the property "text" of "root" is "PUT\ncontent-type: text/plain;charset=UTF-8\n\nhi"
      And the property "bytes" of "root" is "POST\n\nhi!"

    Scenario: A body is read once, as text, JSON or bytes
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string got: ""
          Component.onCompleted: {
            const read = async () => {
              const reply = await fetch("$SERVER/act.json")
              const bytes = await reply.arrayBuffer()
              let again = ""
              try { await reply.text() } catch (e) { again = `${e instanceof TypeError} ${reply.bodyUsed}` }
              root.got = `${bytes.byteLength} ${again}`
            }
            read()
          }
        }
        """
      When 1 second passes
      Then the property "got" of "root" is "35 true true"

    Scenario: A server that cannot be reached rejects with a TypeError
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string got: ""
          Component.onCompleted: {
            fetch("http://127.0.0.1:9/nothing")
              .then(() => { root.got = "answered" })
              .catch(e => { root.got = (e instanceof TypeError) + " " + e.name })
          }
        }
        """
      When 1 second passes
      Then the property "got" of "root" is "true TypeError"
      And the application has nothing scheduled

  Rule: An aborted signal stops the request, and the fetch rejects with its reason

    Scenario: A request aborted after it is sent rejects with an AbortError
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string got: ""
          Component.onCompleted: {
            const controller = new AbortController()
            fetch("$SERVER/act.json", { signal: controller.signal })
              .then(() => { root.got = "answered" })
              .catch(e => { root.got = (e instanceof DOMException) + " " + e.name + " " + e.code })
            controller.abort()
            root.got = "aborted " + controller.signal.aborted
          }
        }
        """
      When 1 second passes
      Then the property "got" of "root" is "true AbortError 20"
      And the application has nothing scheduled

    Scenario: A signal aborted with a reason rejects with that reason, before anything is sent
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string got: ""
          Component.onCompleted: {
            const controller = new AbortController()
            controller.signal.onabort = () => { root.got += "abort " }
            controller.abort("gone")
            controller.abort("twice")
            fetch("$SERVER/echo", { method: "POST", body: "never", signal: controller.signal })
              .catch(e => { root.got += e + " " + controller.signal.reason })
          }
        }
        """
      When 1 second passes
      Then the property "got" of "root" is "abort gone gone"

    Scenario: A signal that times out rejects with a TimeoutError
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string got: ""
          property string later: ""
          readonly property var deadline: AbortSignal.timeout(500)
          Component.onCompleted: {
            root.got = "" + deadline.aborted
            deadline.addEventListener("abort", () => {
              root.got += " " + deadline.reason.name + " " + deadline.reason.code
              fetch("$SERVER/act.json", { signal: deadline }).catch(e => { root.later = e.name })
            })
          }
        }
        """
      When 400 milliseconds pass
      Then the property "got" of "root" is "false"
      When 200 milliseconds pass
      Then the property "got" of "root" is "false TimeoutError 23"
      And the property "later" of "root" is "TimeoutError"
      And the application has nothing scheduled

    Scenario: A signal of any of others aborts with the first one's reason
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string got: ""
          Component.onCompleted: {
            const user = new AbortController()
            const page = new AbortController()
            const either = AbortSignal.any([user.signal, page.signal])
            root.got = "" + either.aborted
            page.abort("left the page")
            user.abort("pressed stop")
            root.got += " " + either.aborted + " " + either.reason
            try { either.throwIfAborted() } catch (e) { root.got += " threw " + e }
          }
        }
        """
      Then the property "got" of "root" is "false true left the page threw left the page"

  Rule: Headers keep a name once, in lower case, and join its values

    Scenario: Headers are read and changed as WHATWG says
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string got: ""
          Component.onCompleted: {
            const h = new Headers([["Accept", "text/plain"], ["X-B", "2"]])
            h.append("x-b", "3")
            h.set("X-A", "1")
            h.delete("accept")
            const seen = []
            h.forEach((value, name) => seen.push(name + "=" + value))
            for (const [name, value] of h.entries()) seen.push(name + ":" + value)
            root.got = seen.join(" ") + " " + h.has("X-A") + " " + h.get("accept")
          }
        }
        """
      Then the property "got" of "root" is "x-a=1 x-b=2, 3 x-a:1 x-b:2, 3 true null"

  Rule: The checker knows fetch's types, and a file of Ljós's API has no XMLHttpRequest

    @headless
    Scenario: What fetch gives is typed, and what json gives is unknown until it is narrowed
      Given the file "Main.qml":
        """qml
        import Ljos.Ui
        Item {
          property string label: ""
          property int status: 0
          function load(): void {
            const controller = new AbortController()
            const run = async (): Promise<void> => {
              const reply = await fetch(new Request("https://example.org/a.json"), { signal: controller.signal, headers: { Accept: "application/json" } })
              status = reply.status
              const given = await reply.json()
              label = given.activity
            }
            run()
            AbortSignal.timeout(5000).throwIfAborted()
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:11:29: error: `activity` has no type: `given` is unknown: narrow it first (`typeof given === "object" && given !== null && "activity" in given`), or decode it with a type guard [ts-no-any]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    @headless
    Scenario: A file of Ljós's API that makes an XMLHttpRequest is told to fetch
      Given the file "Main.qml":
        """qml
        import Ljos.Ui
        Item {
          function load(): void {
            const http = new XMLHttpRequest()
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:4:30: error: `XMLHttpRequest` is not in Ljós's API: use `fetch` [ts-forbidden]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    @headless
    Scenario: A name of the file's own is that one, though fetch has it too
      Given the file "Headers.qml":
        """qml
        import Ljos.Ui
        Item {
          enum Kind { Plain, Bold }
        }
        """
      And the file "Main.qml":
        """qml
        import Ljos.Ui
        Item {
          property string shown: ""
          property int kind: Headers.Bold
          function fetch(name: string): string { return name + "!" }
          function load(): void { shown = fetch("own") }
        }
        """
      When the files are checked
      Then the check reports:
        """
        2 files, 0 errors, 0 warnings
        """

    @headless
    Scenario: TypeScript reads fetch's types as lib.dom's
      Given the file "Main.qml":
        """qml
        import Ljos.Ui
        Item {
          property string label: ""
          function load(): void {
            const run = async (): Promise<void> => {
              const reply: Response = await fetch("https://example.org/a.json", { method: "POST", body: "x", mode: "cors" })
              label = reply.statusText
            }
            run()
          }
        }
        """
      When the files are checked with TypeScript
      Then TypeScript reports the lines:
        """
        Main.qml:6:110: error: Object literal may only specify known properties, and 'mode' does not exist in type 'RequestInit'. [TS2353]
        """

  Rule: ljos migrate rewrites XMLHttpRequest's usual shape into fetch, and keeps every other

    @headless
    Scenario: A request answered in onreadystatechange is a fetch whose reply is awaited
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            property string got: ""
            function load(): void {
                var http = new XMLHttpRequest()
                var url = "https://example.org/api/activity";
                http.open("GET", url, true);

                http.setRequestHeader("Content-type", "application/json");
                http.setRequestHeader("Connection", "close");

                http.onreadystatechange = function() {
                    if (http.readyState == 4) {
                        if (http.status == 200) {
                            got = JSON.parse(http.responseText.toString()).activity
                        } else {
                            console.log("Failed to fetch")
                        }
                    }
                }
                http.send();
            }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (16 edits)
        scripts of ./Main.qml stay JavaScript: typed, ljos check finds 3
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos migrate --write ."
      Then the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        Item {
            property string got: ""
            function load(): void {
                var url = "https://example.org/api/activity";


                fetch(url, { headers: { "Content-type": "application/json", "Connection": "close" } }).catch(() => Response.error()).then(async (http: Response) => {
                    if (http.status == 200) {
                        got = JSON.parse((await http.text()).toString()).activity
                    } else {
                        console.log("Failed to fetch")
                    }
                })
            }
        }
        """

    @headless
    Scenario: A request that returns early until it is done, sends a body and reads bytes is a fetch
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            property int size: 0
            property int status: 0
            Component.onCompleted: {
                const request = new XMLHttpRequest()
                request.responseType = "arraybuffer"
                request.onreadystatechange = () => {
                    if (request.readyState !== XMLHttpRequest.DONE)
                        return
                    status = request.status
                    size = request.response.byteLength + request.response.byteLength
                }
                request.open("POST", "https://example.org/echo")
                request.send("ping")
            }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (11 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
            property int size: 0
            property int status: 0
            Component.onCompleted: {
                fetch("https://example.org/echo", { method: "POST", body: "ping" }).catch(() => Response.error()).then(async (request: Response) => { const requestBytes = await request.arrayBuffer();
                    status = request.status
                    size = requestBytes.byteLength + requestBytes.byteLength
                })
            }
        }
        """

    @headless
    Scenario Outline: A request of another shape keeps the file, and says why
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            property string got: ""
            function load(): void {
                const http = new XMLHttpRequest()
                <line>
                http.onreadystatechange = function() {
                    if (http.readyState === 4)
                        got = <read>
                }
                http.send()
            }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:<at>: <why>
        1 files, 0 lowered, 1 kept
        """

      Examples:
        | line                                                    | read                     | at   | why                                                                                                     |
        | http.open("GET", "https://example.org/a", false)        | http.responseText        | 6:51 | the request waits for its reply (`open`'s third argument is `false`), and `fetch` does not: make the function async and await `fetch` |
        | http.open("GET", "https://example.org/a"); http.onprogress = null | http.responseText | 6:57 | the request's `onprogress` has no `fetch` of its usual shape: write it with `fetch` |
        | http.open("GET", "https://example.org/a")               | http.responseXML         | 9:28 | the request's `responseXML` has no `fetch` of its usual shape: write it with `fetch` |
        | http.open("GET", "https://example.org/a")               | String(http.readyState)  | 9:35 | the request's `readyState` is read where `fetch` has no such thing: write it with `fetch` |
        | http.open("GET", "https://example.org/a"); got = "early" | http.responseText      | 6:52 | code runs between the request's `open` and its `send`, which `fetch` is one call for: write it with `fetch` |
        | http.open("GET", "https://example.org/a")               | String([http])           | 9:31 | the request is handed on, and what is done with it is not known: write it with `fetch` |
        | http.open("GET", "https://example.org/a")               | this.responseText        | 9:23 | the handler reads `this`, which an arrow function does not have: name the request instead |

  Rule: A lowered request fetches what the request fetched

    Scenario: The usual shape of XMLHttpRequest, lowered, fetches the same
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item {
            id: root; objectName: "root"
            property string got: ""
            property string missing: ""
            function load(path, done) {
                const http = new XMLHttpRequest()
                http.onreadystatechange = function() {
                    if (http.readyState !== XMLHttpRequest.DONE)
                        return
                    done(http.status + " " + http.responseText)
                }
                http.open("GET", "$SERVER/" + path)
                http.send()
            }
            Component.onCompleted: {
                load("act.json", r => root.got = r)
                load("missing.json", r => root.missing = r)
            }
        }
        """
      When 1 second passes
      Then the property "got" of "root" is "200 {\"activity\": \"Read a book\", \"n\": 3}"
      And the property "missing" of "root" is "404 Not Found"
