# What this fork adds

Syncly's server is a fork of [synctv-org/synctv](https://github.com/synctv-org/synctv),
MIT, kept under its original licence. Upstream's README describes the server
this is built on; this file covers what is different.

Pair it with [syncly-app](https://github.com/shlkin/syncly-app) and
[syncly-web](https://github.com/shlkin/syncly-web) — the deployment guide lives in the latter.

## 爱的小窝

Eight tables and thirty-one endpoints behind a bound couple's space: the
timeline, albums and media, anniversaries, the shared list, the pet, and the
saved 心动AI pieces. Migration `20260908001_create_couple_nest.sql`; the server
applies its own migrations at startup, so an upgrade needs nothing else.

Three decisions in the schema worth knowing before you change it:

- **Media lives in one table**, not one per owner, because the home shows the
  most recent items across albums *and* timeline entries. Two tables would make
  that a union on every open.
- **Hunger and mood are stored as of an instant** and decayed by the reader.
  There is no job to run, and a pet left alone for a week is hungry the moment
  someone opens the app rather than whenever a sweeper last ran.
- **Streaks are derived from the check-in rows**, not kept as a counter. A
  counter cannot say which day was missed, and drifts the first time a write is
  lost.

Every query is keyed on the space id and membership is checked in the service
layer, so there is no request shape that returns another pair's anything.

`/api/user/couple/nest/media-objects/{id}` streams a photo's bytes to a member
and answers "not found" to anyone else — private photos should not be reachable
by a link alone.

## A bound couple may message without also being friends

`UserService::may_message` now accepts a couple-space membership as well as a
friendship. 悄悄话 in the app is the *same* direct thread the messages tab
shows; requiring a pair who have already bound their accounts to also send each
other a friend request was a hoop with nothing behind it. Strangers are still
refused.

## 影视 subscription sources

`vod_sources` and `GET /api/vod/sources`: an administrator publishes MacCMS-JSON,
MacCMS-XML or m3u catalogues once, and every account sees them. Clients merge
these with whatever the device imported by hand.

No source list ships in this repository, and the server fetches nothing on its
own — it stores addresses and hands them to clients.

## Gifts transfer points

`send_gift` debits the sender and credits the recipient in one transaction,
writing both halves to the ledger. Upstream burned them. Two accounts can
therefore pass the same points back and forth at no cost — deliberately, since
both halves are auditable. `total_earned` is *not* credited: it means "earned
from the system", and a pair could otherwise inflate it without limit.

## Media provider fixes

Bilibili's WAF answers **HTTP 412** to a desktop-browser user agent arriving
from a datacentre without a browser's TLS fingerprint, so the client sends
`SyncTV/1.0` instead. Do not "fix" this back to a browser string. The media CDN
does not care about the agent but does require `Referer: https://www.bilibili.com`.

## Building

Unchanged from upstream, including the Docker and Helm paths. Note that the
workspace uses sqlx's compile-time query checking, so a build needs either a
reachable `DATABASE_URL` with the schema applied or the checked-in `.sqlx`
offline data.
