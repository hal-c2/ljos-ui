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

`fetch` takes an address and gives a promise of a `Response`. The body is read once, as text, JSON or bytes, each a promise of its own. What `json()` gives is `unknown`: the script narrows it before it reads a member.

```qml
import Ljos.Ui
import Ljos.Core

Item {
    id: root
    property string activity: ""

    function load(): void {
        const run = async (): Promise<void> => {
            const reply = await fetch("https://example.org/activity.json")
            if (!reply.ok) {
                root.activity = `${reply.status} ${reply.statusText}`
                return
            }
            const given: unknown = await reply.json()
            if (typeof given === "object" && given !== null && "activity" in given && typeof given.activity === "string") {
                root.activity = given.activity
            }
        }
        run()
    }

    Component.onCompleted: load()
}
```

A status the server sends, `404` or `500`, is a `Response` whose `ok` is `false`: `fetch` does not reject for it. `text()` gives the body as a string and `arrayBuffer()` as bytes; reading a body a second time rejects with a `TypeError`, and `bodyUsed` says it was read.

## Sending a body and headers

The second argument says what is sent: `method`, `headers` and `body`. Headers are a `Headers`, an object of names or a list of pairs; a name is kept once, in lower case, and the values given for it are joined with `, `. Headers the platform owns, such as `Connection` or `Content-Length`, are not sent. A body of text is sent as `text/plain;charset=UTF-8` unless the headers say otherwise, and a `Uint8Array` or an `ArrayBuffer` is sent as it is.

```qml
import Ljos.Ui
import Ljos.Core

Item {
    id: root
    property string saved: ""

    function save(name: string): void {
        const headers = new Headers({ "Content-Type": "application/json" })
        fetch("https://example.org/names", { method: "POST", headers, body: JSON.stringify({ name }) })
            .then((reply: Response) => reply.text())
            .then((text: string) => { root.saved = text })
    }

    Component.onCompleted: save("Ada")
}
```

## Stopping a request, and deadlines

An `AbortController`'s `signal`, given as `signal`, stops the request when the controller aborts: the fetch rejects with the signal's reason, a `DOMException` named `AbortError` unless the script gave its own. `AbortSignal.timeout(ms)` is a signal that aborts by itself after `ms` milliseconds, with a `TimeoutError`, and `AbortSignal.any([...])` aborts with the first of several.

```qml
import Ljos.Ui
import Ljos.Core

Item {
    id: root
    property string state: "idle"

    function load(): void {
        const user = new AbortController()
        const signal = AbortSignal.any([user.signal, AbortSignal.timeout(5000)])
        const run = async (): Promise<void> => {
            try {
                const reply = await fetch("https://example.org/slow", { signal })
                root.state = `${reply.status}`
            } catch {
                root.state = signal.aborted ? "stopped" : "unreachable"
            }
        }
        run()
        user.abort()
    }

    Component.onCompleted: load()
}
```

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

The lowered file keeps every line, so the statements the call gathers leave blank lines. Any other shape, a request that waits for its reply or a handler that reads `this`, keeps the file, and `ljos migrate` says what of it has no `fetch`.
