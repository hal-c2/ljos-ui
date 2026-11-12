# Network

What a file asks of the network: `XMLHttpRequest`, written as `fetch`
([DIALECT.md §4.19](../../DIALECT.md#419-fetching-fetch-and-task)). Written only by the lane
building this area.

Built:

- `XMLHttpRequest`'s usual shape (a local that is opened, given headers and
  a `responseType`, answered in an `onreadystatechange` that waits for
  `readyState` 4, and sent) is one `fetch(…).catch(() => Response.error())
  .then(async (http: Response) => {…})`, with `responseText`, `response`,
  `status`, `statusText`, `responseURL` and `getResponseHeader` read from
  the reply.
- Every other shape keeps the file, and says what of it has no `fetch`.

Remaining:

- A request that waits for its reply (`open`'s third argument `false`),
  written as an `await` in an async function.
- `onload`, `onerror`, `ontimeout` and `timeout`, written with a signal.
