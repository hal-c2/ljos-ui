---
title: Fetching from the Network
description: Ask a server for data with fetch, read the Response as JSON, text or bytes, stop a request with an AbortController or a timeout, and tell a failed request from a reply that is not ok.
section: data
order: 7
---

# Fetching from the Network

After reading this page, you will know how a script asks a server for data with `fetch`, how it reads the reply, how it stops a request or gives it a deadline, and what an error is.

A script fetches as a web page does: `fetch`, `Request`, `Response`, `Headers`, `AbortController` and `AbortSignal`, with the types and the meaning WHATWG gives them. The request runs on a thread of the platform, and the application does nothing while it waits: no timer and no frame. Its reply wakes the application, settles the promise and runs what waited on it.

## A request and its reply

`fetch` takes an address and gives a promise of a `Response`. The body is read once, as text, JSON or bytes, each a promise of its own. What `json()` gives is `unknown`: the script narrows it before it reads a member. This example reads `activity.json`, a file beside it:

```json file=activity.json group=activity
{"activity": "Walk to the lighthouse"}
```

```qml group=activity
import Ljos.Ui
import Ljos.Core

Text {
    id: root
    width: 280
    wrap: Text.WordWrap
    text: "Loading"

    function load(): void {
        const run = async (): Promise<void> => {
            try {
                const reply = await fetch("activity.json")
                if (!reply.ok) {
                    root.text = `${reply.status} ${reply.statusText}`
                    return
                }
                const given: unknown = await reply.json()
                if (typeof given === "object" && given !== null && "activity" in given && typeof given.activity === "string") {
                    root.text = given.activity
                }
            } catch (error) {
                if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") {
                    root.text = error.message
                }
            }
        }
        run()
    }

    Component.onCompleted: load()
}
```

On ljos.dev, and in a browser, a relative address is read against the page, so the example shows "Walk to the lighthouse". On the desktop an address names a server, `https://…`: a relative one is not an address there, so the same example shows the message of the `TypeError` that `fetch` rejects with.

A status the server sends, `404` or `500`, is a `Response` whose `ok` is `false`: `fetch` does not reject for it. `text()` gives the body as a string and `arrayBuffer()` as bytes; reading a body a second time rejects with a `TypeError`, and `bodyUsed` says it was read.

## Sending a body and headers

The second argument says what is sent: `method`, `headers` and `body`. Headers are a `Headers`, an object of names or a list of pairs; a name is kept once, in lower case, and the values given for it are joined with `, `. Headers the platform owns, such as `Connection` or `Content-Length`, are not sent. A body of text is sent as `text/plain;charset=UTF-8` unless the headers say otherwise, and a `Uint8Array` or an `ArrayBuffer` is sent as it is.

A `Request` holds the same two arguments, and `fetch(request)` sends it. This example makes one and shows what it would send, without sending it:

```qml
import Ljos.Ui
import Ljos.Core

Text {
    id: root
    width: 280
    text: ""

    function describe(name: string): void {
        const headers = new Headers({ "Content-Type": "application/json" })
        const request = new Request("names", { method: "POST", headers, body: JSON.stringify({ name }) })
        root.text = `${request.method} ${request.url}\ncontent-type: ${request.headers.get("content-type") ?? ""}`
    }

    Component.onCompleted: describe("Ada")
}
```

Its first line is `POST` and the address: `names` on the desktop, which keeps the address as given, and in a browser the whole address `names` makes beside the page. The second reads `content-type: application/json`: the header given wins over the one the body would set.

## Stopping a request, and deadlines

An `AbortController`'s `signal`, given as `signal`, stops the request when the controller aborts: the fetch rejects with the signal's reason, a `DOMException` named `AbortError` unless the script gave its own. `AbortSignal.timeout(ms)` is a signal that aborts by itself after `ms` milliseconds, with a `TimeoutError`, and `AbortSignal.any([...])` aborts with the first of several.

The example asks for `slow.json`, a file beside it:

```json file=slow.json group=abort
{ "answer": "too late" }
```

```qml group=abort
import Ljos.Ui
import Ljos.Core

Text {
    id: root
    width: 200
    text: "idle"

    function load(): void {
        const user = new AbortController()
        const signal = AbortSignal.any([user.signal, AbortSignal.timeout(5000)])
        const run = async (): Promise<void> => {
            try {
                const reply = await fetch("slow.json", { signal })
                root.text = `${reply.status}`
            } catch (error) {
                if (typeof error === "object" && error !== null && "name" in error && error.name === "AbortError") {
                    root.text = "stopped"
                } else {
                    root.text = "unreachable"
                }
            }
        }
        run()
        user.abort()
    }

    Component.onCompleted: load()
}
```

The user's controller aborts as soon as the request is made, so the fetch rejects with an `AbortError` and the text reads "stopped". The `catch` asks the error's `name`, not `signal.aborted`, so a request that could not be made reads "unreachable" even after an abort.

## What an error is

`fetch` rejects in two cases only:

- the request could not be made: the server cannot be reached, the address is not one, or the method is not one the platform sends. It rejects with a `TypeError` whose message begins `fetch failed:`.
- the signal aborted: it rejects with the signal's reason.

Everything the server answers is a `Response`. `Response.error()` is a reply of no request, whose `status` is `0` and whose body is empty: `fetch(url).catch(() => Response.error())` gives every request a `Response`, which is what `ljos migrate` writes for `XMLHttpRequest`.

## On the web

An app built for the web fetches with the browser's own `fetch`, which has every name on this page with the same meaning, so a script that checks runs the same there. Two things are the browser's: a request to another origin needs the server's leave (CORS), which the desktop does not ask for, and the browser leaves out a few more headers, such as `Host`, than it lets a page set.

## From XMLHttpRequest

A file of Ljós's API has no `XMLHttpRequest`: `ljos check` says `` `XMLHttpRequest` is not in Ljós's API: use `fetch` [ts-forbidden] ``. A file written against Qt's API keeps it, and `ljos migrate` writes its usual shape as one `fetch`: a request that is opened, given headers, answered in an `onreadystatechange` that waits for `readyState` 4, and sent.

```javascript
var http = new XMLHttpRequest()
http.open("GET", "https://example.org/activity.json", true)
http.onreadystatechange = function() {
    if (http.readyState === 4) {
        got = http.status + " " + http.responseText
    }
}
http.send()
```

becomes

```javascript
fetch("https://example.org/activity.json").catch(() => Response.error()).then(async (http: Response) => {
    got = http.status + " " + (await http.text())
})
```

`ljos migrate --write` takes out the lines the statements the call gathers leave empty; `--compat lower` keeps them, so that its findings point at the file as written. Any other shape, a request that waits for its reply or a handler that reads `this`, keeps the file, and `ljos migrate` says what of it has no `fetch`.
