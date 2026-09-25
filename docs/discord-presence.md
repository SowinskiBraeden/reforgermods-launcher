# Discord Rich Presence — setup

**Status: live.** Application id `1552757014474784898` is set in
`src-tauri/src/presence.rs`.

The launcher appears on a user's Discord profile the way DZSA Launcher does for
DayZ — "Playing reforgermods.net launcher", with artwork, the server being
viewed, and a button linking to the site.

The application id supplies the *name* Discord displays, so the Discord
application must be named exactly `reforgermods.net launcher`. Artwork still has
to be uploaded for the large image to appear; see step 3.

## What it looks like

```text
  ┌────┐  reforgermods.net launcher
  │ RM │  Viewing [OGR2] Old Guard Revival 2
  └────┘  12/128 players
          [ Get the launcher ]
```

- **Application name** — from the Discord application. This is the headline and
  the reason the application must be created with exactly the right name.
- **Details / state** — from `rfm_core::presence`.
- **Large image** — the asset keyed `logo` in the application's Rich Presence art.
- **Button** — "Get the launcher" → `https://reforgermods.net`.

## Remaining setup

1. ~~Create the application~~ — done, id `1552757014474784898`.
2. **Confirm the application name is exactly `reforgermods.net launcher`.** This
   string is what every user's friends list shows; it cannot be changed
   per-client, and it is the whole promotional value of the feature.
3. **Upload the artwork.** Under **Rich Presence → Art Assets**, add
   `brand/reforger-mods-app-icon.png` with the key **`logo`**, matching
   `LARGE_IMAGE_KEY` in `src-tauri/src/presence.rs`. Until this exists the text
   and button still show, but the card has no image.

The application id is a public identifier, not a secret — it ships in every
Discord game integration and is visible in any client's traffic. It is safe in
the repository. Do **not** put the application's *client secret* or bot token
anywhere near this project; Rich Presence needs neither.

### Trying it without editing the source

```sh
RFM_DISCORD_APP_ID=123456789012345678 ./reforgermods-launcher
```

## Behaviour

| Situation | Result |
| --- | --- |
| No application id configured | Presence is disabled; no thread, no socket. |
| Discord not running | Connection fails quietly, retried at most every 30s. |
| Discord restarts | Next update reconnects and republishes the current state. |
| User turns it off in Settings | Presence is cleared immediately. |

Presence runs on its own thread and is never awaited from a command, so Discord
being slow, absent or broken cannot stall the UI.

## Privacy

Presence is **on by default**, since being visible is the point, but it is a
visible toggle in Settings because it broadcasts the name of the server being
viewed to the user's Discord friends. Nothing else is published: no account, no
install identifier, no IP, no mod list.

## Why the text rules are tested

`crates/rfm-core/src/presence.rs` builds the strings and is unit tested, because
server names are untrusted text from the API and Discord is unforgiving:

- `details` and `state` cap at 128 characters, and Discord **silently drops** a
  field under two characters rather than reporting an error.
- Truncation is by character. Cutting a multi-byte glyph in half yields invalid
  UTF-8 and Discord rejects the entire payload — so a server named in Japanese
  or full of flag emoji would blank the presence rather than shorten it.
- Control characters are stripped and whitespace collapsed, so a server name
  containing newlines cannot reformat the presence card.

The "reforgermods.net launcher" line is ours and is never taken from remote
data, so a server cannot name itself something that impersonates the launcher.
