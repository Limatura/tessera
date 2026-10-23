# Webhooks

A webhook is a URL told when a share is created, downloaded or expires. There are two kinds:

- **An account's own**, under `/api/v1/webhooks`. It hears about that account's shares only. Any account that can see its shares (`shares:read`) can add up to ten.
- **The instance's**, under `/api/v1/admin/webhooks`, added by someone with `settings:write`. It hears about every share, anonymous ones included. Also up to ten.

Adding or removing one needs a browser session, not an API token: a token that leaked would otherwise be a way to keep hearing about the account.

## Events

| Event | When |
| --- | --- |
| `share.created` | A share is stored. A share with several files is created with its first, so only that one is listed. |
| `share.downloaded` | A retrieval is counted, the same moment `use_count` goes up. A download resumed with its ticket is not counted again. |
| `share.expired` | It is past its expiry, or has been retrieved as many times as it allowed. Noticed within thirty seconds, since a share has no moment of expiring, only a time after which it has. Sent once. |

A share switched off by an administrator, or deleted, sends nothing.

## The body

JSON, shaped as [Standard Webhooks](https://www.standardwebhooks.com) describes:

```json
{
  "type": "share.downloaded",
  "timestamp": "2026-10-23T09:12:44Z",
  "data": {
    "id": "01K8…",
    "code": "0ZPQ4M7K",
    "url": "https://files.example.com/s/0ZPQ4M7K",
    "kind": "file",
    "title": null,
    "encrypted": false,
    "size_bytes": 5,
    "files": [{ "name": "notes.txt", "size_bytes": 5 }],
    "use_count": 1,
    "use_limit": 3,
    "expires_at": "2026-10-30T09:10:02Z",
    "owner_id": null,
    "download": { "address": "203.0.113.7", "user_agent": "curl/8.9.1" }
  }
}
```

`url` is `null` unless `server.public_url` is set: the event is sent from the background, with no request to take the address from. An encrypted share's names are `null`, since the server cannot read them.

`download` is only on `share.downloaded`, only for the instance's webhooks, and only while `share.access_log.enabled` is on. Who downloaded something is what the access log records, and only administrators can read that; an account's webhook would otherwise be a way around both.

A webhook added with `"format": "discord"` is sent a message a Discord channel's webhook URL will post instead: one embed with what happened, the share's name and code, its size and downloads.

## Checking it came from this server

Every request carries three headers:

| Header | |
| --- | --- |
| `webhook-id` | The event's id. The same on every retry, so a receiver can drop one it has seen. |
| `webhook-timestamp` | When this attempt was sent, in Unix seconds. |
| `webhook-signature` | `v1,` and the base64 of an HMAC-SHA256 over `{webhook-id}.{webhook-timestamp}.{body}`, keyed with the base64-decoded part of the secret after `whsec_`. |

The secret is returned once, when the webhook is added. Any Standard Webhooks library checks the signature; the test `signs_as_standard_webhooks_does` checks ours against the specification's example.

## Delivery

Events are queued in the database when they happen and sent by a background task, woken at once and every thirty seconds. A request never waits on a receiver.

Anything but a `2xx` within ten seconds is a failure. A failed event is sent again after 30 seconds, 2 minutes, 10 minutes, an hour, 6 hours and a day, then given up on. Redirects are not followed.

`GET …/webhooks/{id}/deliveries` lists a webhook's last fifty events: `pending`, `delivered` or `failed`, with the attempts and the last answer. Finished events are kept a week.

## An account's webhooks stay off this network

A webhook is a request this server makes to a URL someone else chose. An account could aim one at `localhost`, the router, or a cloud provider's metadata address and use the server to reach what only it can. So an account's webhooks only go to public addresses: a URL naming a private address or `localhost` is refused when it is added, and on every send the name is resolved and only its public addresses are connected to, so a name that resolves somewhere private later is still caught. No proxy is used for them, since a proxy would resolve the name itself.

The instance's webhooks are not limited. Whoever runs the instance already reaches its network, and a receiver on it is a reasonable thing to want.
