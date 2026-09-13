# Message

HiveMe publishes JSON. This document defines the envelope, its payload, the
encrypted variant, and the compatibility rules that let old and new readers share a
broker.

The machine readable schema is [`schemas/message.schema.json`](../../schemas/message.schema.json),
generated from the `hiveme-core::message` types with `cargo xtask schema`. The
examples below are extracted and validated by `cargo xtask check-spec`. Each
compatibility rule names the fixture that proves it.

## Goals

- A JSON payload on the wire, readable with `jq` and by third party tools.
- Backward compatible: a new reader reads an old message.
- Forward compatible: an old reader reads a new message, degrading gracefully.
- One envelope shape for plaintext and encrypted messages, so a reader can route on
  the header before touching the body.
- Third party JSON and plain text are still displayed, never dropped.

## Plaintext envelope

```json hiveme:message
{
  "v": 1,
  "id": "018f6b1e-7c2a-7d3e-9f1a-4b2c8d9e0f11",
  "ts": "2026-09-12T09:41:23.512Z",
  "type": "message",
  "sender": {
    "id": "0f7a1c2e-5d4b-4a6e-9c1d-2b3e4f5a6b7c",
    "name": "sams-macbook",
    "app": "hmc",
    "appVersion": "0.1.0"
  },
  "payload": {
    "level": "info",
    "title": "Build finished",
    "body": "hiveme v0.1.0 built in 42s",
    "data": { "duration_s": 42 }
  }
}
```

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `v` | integer | yes | Envelope major version, `1` today. Bumped only for an incompatible change. |
| `id` | string | yes | UUID v7 recommended. Used to de-duplicate, and to match a message sent by the GUI with its broker echo. |
| `ts` | string | yes | RFC 3339 with a timezone and millisecond precision, from the producer's clock. |
| `type` | string | no, default `message` | Open enum. Unknown types are shown as raw JSON. Reserved: `message`, `ack`, `presence`. |
| `sender` | object | no | See below. |
| `payload` | object | yes when `enc` is absent | See [Payload](#payload). |
| `enc` | object | no | Present only on encrypted messages. See [Encrypted envelope](#encrypted-envelope). |
| `ciphertext` | string | yes when `enc` is present | base64url without padding. |
| `replyTo` | string | no | The `id` of another message. Reserved for threading. |
| `ttlSecs` | integer | no | Mirrors the MQTT message expiry so a reader can show staleness. |

`sender` carries `id` (the device UUID), `name`, `app` (`hmc`, `hmg`, or any string),
and `appVersion`. Every field inside is optional and unknown keys are allowed.

## Payload

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `body` | string | yes | The main text. May be empty when `data` carries the content. |
| `title` | string | no | Used as the notification title when present. |
| `level` | string | no, default `info` | Case-insensitive open enum: `debug`, `info`, `success`, `warn`, `error`. Normalized to lowercase. Unknown values display with the Info fallback and the lowercase level name. |
| `data` | object | no | Free form JSON for scripts. Rendered as a collapsible tree in the GUI. |
| `contentType` | string | no | A hint for `body`, such as `text/markdown`. Defaults to `text/plain`. |

Levels are case-insensitive when read and normalized to lowercase in the shared
model, serialized JSON, and the database's `level` field. Only display labels use
an initial capital. Unknown level names are preserved in lowercase.

`success` marks a successful result and uses the GUI's MUI success palette. The
shared `success.json` fixture verifies that both readers recognize it, and Rust
preserves it when serializing. As with every level, it does not change the MQTT topic.

## Encrypted envelope

```json hiveme:message-encrypted
{
  "v": 1,
  "id": "018f6b1e-7c2a-7d3e-9f1a-4b2c8d9e0f12",
  "ts": "2026-09-12T09:41:23.512Z",
  "type": "message",
  "sender": { "id": "0f7a1c2e-5d4b-4a6e-9c1d-2b3e4f5a6b7c", "name": "sams-macbook", "app": "hmc" },
  "enc": {
    "alg": "A256GCM",
    "kid": "k-2026-09",
    "iv": "u2m1xwK7Ck3NoMbz"
  },
  "ciphertext": "8Qy0m5jI1n1F0y7b3z9K2Qw3e5r7t9y1u3i5o7p9a1s3d5f7g9h1j3k5l7"
}
```

- `enc.alg` is an open enum. `A256GCM` (AES-256-GCM) is the first algorithm; `XC20P`
  (XChaCha20-Poly1305) is reserved.
- `enc.kid` selects the pre-shared key.
- `enc.iv` is a base64url nonce, 96 bits for `A256GCM`.
- `ciphertext` is the AEAD output including the tag, base64url. The plaintext is the
  UTF-8 JSON of the `payload` object.
- `payload` is absent when `enc` is present. A reader that sees both prefers `enc`
  and ignores `payload`.

## MQTT 5 properties

Publishers set these, and a reader may use them before parsing the body:

- Content type: `application/json`.
- User property `hiveme-v`: `"1"`.
- User property `hiveme-enc`: the `enc.alg` value, on encrypted messages only.
- Message expiry interval: set only when `ttlSecs` is present.

An MQTT 3.1.1 reader ignores all of these. Everything they convey is also inside the
JSON.

## Compatibility rules

Both readers, `hiveme-core::message` in Rust and `src/lib/message.ts` in TypeScript,
follow these rules. Fixtures live in `crates/hiveme-core/tests/fixtures/message/` and
the TypeScript tests read the same files.

| # | Rule | Fixture |
|---|------|---------|
| 1 | Unknown keys at any level are ignored, never rejected. | `unknown_fields.json` |
| 2 | Missing optional keys take their documented defaults. | `minimal.json` |
| 3 | Open enums (`type`, `payload.level`, `enc.alg`) never fail parsing. An unknown `enc.alg` or an unknown `kid` produces a "cannot decrypt" placeholder that still shows `sender`, `ts`, and the topic. | `unknown_level.json`, `unknown_alg.json` |
| 4 | A `v` greater than the supported version parses its known fields, is marked `newerVersion` in the UI, and is never dropped. | `newer_version.json` |
| 5 | Non envelope input falls back through the tiers below. | `raw_json.json`, `raw_text.txt`, `invalid_utf8.bin` |
| 6 | Writers always emit `v`, `id`, `ts`, `type`, and `sender`, so future readers can rely on them. | asserted by the builder tests |
| 7 | A breaking change (renaming a field, changing a type, changing semantics) requires `v: 2`, a new `message-v2.schema.json`, and readers that accept both. | n/a |

### Parse tiers

| Tier | Stored as | Input | Result |
|------|-----------|-------|--------|
| A, envelope | `envelope` | A JSON object with an integer `v` and either `payload` or `enc` | Parsed envelope |
| B, raw JSON | `json` | Any other valid JSON | Shown as a JSON tree. The notification body is the compact JSON truncated to 200 characters. |
| C, raw text | `text` | Bytes that are valid UTF-8 | Shown as text |
| C, raw bytes | `bytes` | Anything else | Shown as a size label, and as hex in the message view |

A document that claims to be an envelope, by carrying an integer `v` and a `payload` or
`enc` key, but whose shape is wrong falls back to tier B rather than being rejected. The
same is true of a combination [the envelope forbids](#plaintext-envelope), such as an
`enc` with no `ciphertext`.

The "Stored as" column is the `tier` value in the history database; see
[gui.md](gui.md#storage).

Nothing is ever discarded.

## Schema shape

The generated schema expresses the envelope as `oneOf` two variants sharing a
header, with `additionalProperties: true` on every object:

```
Message   = Header & (Plain | Encrypted)
Header    = { v: integer >= 1, id: string, ts: date-time, type?: string, sender?: Sender, replyTo?: string, ttlSecs?: integer }
Plain     = { payload: Payload }                      # enc must be absent
Encrypted = { enc: Enc, ciphertext: string }          # payload must be absent
Payload   = { body: string, title?: string, level?: string, data?: object, contentType?: string }
Enc       = { alg: string, kid: string, iv: string }
Sender    = { id?: string, name?: string, app?: string, appVersion?: string }
```

In Rust, `Message` is a struct with `payload: Option<Payload>`, `enc: Option<Enc>`,
and `ciphertext: Option<String>`, plus a `validate()` method, because serde's
untagged enums produce poor error messages. The schema generator post-processes the
derived schema to add the `oneOf` constraint.

## Encryption

**Designed, not implemented.** Phase 6 of
[the initialization plan](../plans/plan-initialization.md) builds it. This section
exists so that the envelope and the config do not have to change when it lands.

### Threat model

- Protects `payload` from the broker operator and from anyone who holds the broker
  credentials but not the HiveMe key.
- Does not hide topic names, `sender`, `ts`, `id`, message sizes, or timing.
- Does not authenticate the sender beyond "holds the shared key". Anyone with the key
  can forge a message.
- Replay: receivers de-duplicate by `id` and may reject a message whose `ts` is older
  than a configurable window. Full replay protection is out of scope.

### Algorithm

- `A256GCM`: AES-256-GCM with a 96 bit random nonce per message and a 128 bit tag.
- Message key: `HKDF-SHA256(ikm = secret, salt = "hiveme/v1", info = "msg:" || kid)`,
  32 bytes. Deriving rather than using the secret directly gives domain separation for
  future uses of the same secret.
- Plaintext: the UTF-8 bytes of the serialized `payload` object.
- Associated data: the compact UTF-8 JSON array
  `[v, id, ts, sender.id or "", type, alg, kid]`. Binding the header this way stops a
  ciphertext being moved to another id, sender, or key.
- Nonce reuse is the main risk with GCM. Random 96 bit nonces are safe well below
  2^32 messages per key, and the rotation policy keeps keys far under that. `XC20P` is
  reserved for a future variant with 192 bit nonces.

### Keys

- A secret is 32 random bytes, base64url encoded.
- A config entry is
  `{ "kid": "k-2026-09", "alg": "A256GCM", "secret": "<base64url>", "createdAt": "2026-09-12", "state": "Active" }`.
  `state` is `Active` (used for sending) or `Retired` (decrypt only). Exactly one key
  is `Active` whenever `mode` is not `Off`.
- Distribution is manual and out of band: the user copies the key entry into the
  config on every device. There is no key exchange over MQTT. Adding one would need
  the asymmetric model, which the envelope can express later through `enc.alg` and a
  future `enc.recipients` field.
- Rotation: add a new `Active` key, mark the old one `Retired`, deploy to every
  device, then delete the retired key once history older than `retentionDays` is gone.
  Receivers select the key by `kid`, so old messages stay readable while the retired
  key exists.
- Storage is plain text in the config at first, with the same `*Ref` mechanism as
  `broker.passwordRef` for keychain storage later.

### Modes

| Mode | Sending | Receiving |
|------|---------|-----------|
| `Off` | plaintext | accepts both |
| `Opportunistic` | encrypted | accepts both |
| `Required` | encrypted | accepts both, flags plaintext envelopes in the UI |

`Required` flags rather than drops, because the GUI is deliberately lenient.

### What exists today

- The `Enc`, `EncryptionConfig`, and `KeyEntry` types, included in both schemas.
- A parser that recognizes an encrypted envelope and returns
  `Message { enc: Some(_), payload: None }`.
- A GUI bubble that renders a lock icon with "encrypted (key k-2026-09)" plus the
  sender and time.
- A refusal to publish while `encryption.mode` is not `Off`: `hmc` exits 3 and `hmg`
  shows a snackbar, both saying that encryption is not implemented yet. A half
  configured setup must not silently send plaintext.
- No cryptography crate is a dependency yet.
