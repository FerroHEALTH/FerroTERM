<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Problem types

FerroTERM answers every error outside its FHIR bases as problem details,
the JSON document of [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457),
with the media type `application/problem+json`. That covers the server's
admin listener (`POST /reload`), a path of the FHIR listener that names no
FHIR base, the [sync service's admin listener](sync.md), and the refusals
the HTTP stack writes itself on those routes: a request past its timeout
(`408`), a body over the limit (`413`), a server at its request limit (`429`
or `503`) and a handler that failed (`500`). Under `/r4`, `/r4b`, `/r5` and
`/r6` every error stays the FHIR `OperationOutcome` the FHIR specification
prescribes.

A problem looks like this:

```json
{
  "type": "https://ferroterm.eu/docs/operate/problems.html#not-permitted",
  "title": "The credential does not permit this request",
  "status": 403,
  "detail": "the credential carries no scope that permits this request"
}
```

- `type` names the problem type. Each FerroTERM type is a link to its
  section on this page. `about:blank` means the status code says all there
  is to say, and `title` is then the status phrase, such as `Not Found`.
- `title` is the type's summary, the same for every occurrence.
- `status` is the HTTP status of the answer.
- `detail` says what happened, in a fixed sentence; a failed reload says why
  the served set could not be rebuilt.
- `instance`, on a refusal of the stack and a failed handler, is the
  request id as a `urn:uuid:` URI, the id the request log names.

A problem never quotes the request: no token, credential, query value or
path segment you sent appears in it, and it carries no stack trace and no
secret, so a problem is safe to show and to log. The `X-Request-Id` header of
the answer names the request when you report it. Every problem answer of the
admin listeners carries `Cache-Control: no-store`.

## The types

| Type | Status | When |
|---|---|---|
| [`unauthenticated`](#unauthenticated) | `401` | The request carries no valid credential. |
| [`not-permitted`](#not-permitted) | `403` | The credential does not permit the route. |
| [`authorization-unavailable`](#authorization-unavailable) | `503` | The credential could not be checked. |
| [`reload-failed`](#reload-failed) | `500` | The server could not rebuild its served set. |
| [`run-not-found`](#run-not-found) | `404` | The sync service holds no such run. |
| [`records-unreadable`](#records-unreadable) | `500` | The sync service could not read its run records. |

## `unauthenticated`

The request carries no valid credential: no `Authorization` header, a token
the issuer did not sign or that has expired, or a Basic user and password
that do not match. Title: "The request carries no valid credential". The
answer carries the `WWW-Authenticate` challenge of the listener. Send the
request again with a valid access token or Basic credential.

## `not-permitted`

The credential is valid and carries no scope that permits the route. Title:
"The credential does not permit this request". The server's admin listener
asks for the scope `[auth] product_scope` names (`__ferroterm.admin` by
default), the sync service's for its own (`__ferroterm.sync` by default).

## `authorization-unavailable`

The credential could not be checked, because the issuer's keys or its
introspection endpoint could not be reached. Title: "The credential could
not be checked". Nothing about the request was decided; send it again once
the issuer answers.

## `reload-failed`

`POST /reload` could not rebuild the served set from the artifacts the
server is configured with. Title: "The served set could not be rebuilt; the
previous one still answers". The `detail` says what failed, such as an
artifact that does not open. The previous set keeps answering every FHIR
request, so fix the artifact and reload again.

## `run-not-found`

`GET /runs/{id}` names a run the sync service has no record of, or an id
that is no run id. Title: "The sync service holds no such run". `GET /runs`
lists the runs it holds.

## `records-unreadable`

The sync service could not read its run records from its `records`
directory. Title: "The sync service could not read its run records". The
service log names the file and the reason; check the directory's
permissions and free space.
