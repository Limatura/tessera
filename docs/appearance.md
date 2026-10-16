# Appearance

Three names decide how the site looks: a mode, a neutral theme and an accent.

```json
{ "mode": "dark", "theme": "sepia", "accent": "rose" }
```

| | Values |
| --- | --- |
| `mode` | `light`, `dark`, `system` |
| `theme` | `default`, `graphite`, `sepia` |
| `accent` | `violet`, `rose`, `orange`, `amber`, `green`, `teal`, `blue` |

Code: `tessera_core::appearance` (the lists), `api/preferences.rs` (one person's), `settings.rs` (the instance's), `assets.rs` (getting them onto the page).

## No colour is ever stored

Each name selects a block of CSS that ships with the frontend, switched by an attribute on `<html>`:

```html
<html lang="zh-Hant" data-mode="dark" data-theme="sepia" data-accent="rose" class="dark">
```

The server never learns what `rose` looks like. That is the point of the split, and it buys two things: colours are a design decision that travels with the design through review, and a value that can only ever be one of the names above cannot carry a stylesheet into an HTML attribute.

Adding a colour is therefore two changes that land together — a variant in `tessera_core::appearance` and the CSS that answers to it.

## Two layers

The instance's answer is three settings, `appearance.mode`, `appearance.theme` and `appearance.accent`, changed from the panel like any other. One person's answer is a row of `user_preferences`, read and written through `GET`/`PUT /api/v1/me/preferences`.

A person's own choice wins where they made one. Every field of theirs is optional, and absent means *no opinion*: somebody who picked an accent and left the rest alone keeps following the instance for mode and theme, including after the operator changes them. Storing the instance's values alongside their one choice would freeze the other two at whatever they happened to be that day, with no way to tell that apart from having chosen them.

`PUT` replaces the whole object. Two devices editing at once is possible and does not matter: the loser sees a colour they did not pick and clicks again, which is cheaper than the version numbers it would take to do better.

## Why the instance's answer is on the page already

`index.html` arrives with the attributes on it, spliced in by `assets.rs`. Otherwise the page loads, paints, and only then does a script discover what the operator chose and repaint — a white flash on every navigation of a dark instance.

Only the *instance's* value goes in. A signed-in person's own choice is applied by the frontend from what it has stored locally. Doing it here would mean resolving a session on the static-asset path and an `index.html` that differs per visitor, so no two of them could share a cached copy.

`class="dark"` is what the stylesheet's dark variant hangs off, so it appears only for an instance that chose dark outright. `data-mode` carries the choice itself, because "the operator picked light" and "the operator left it to the device" are different instructions and an absent class cannot tell them apart.

The page is `no-cache` and its entity tag is computed over what was actually sent, so changing the setting shows on the next load with nothing to purge.

## Where a bad name stops

At the edge, both times. `PUT /me/preferences` refuses a name outside the lists with `400`; the panel refuses one the same way, because `Config` holds the same types and a setting that will not parse is not stored.

Checking there rather than at the point of rendering matters: by then the value has been stored and whoever wrote it is gone.
