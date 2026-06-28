# hiptty API Reference

Headless CLI for www.4d4y.com (Discuz). Default output is JSON (`schema_version: 1`); pass `--human` for readable text.

## Configuration

| Item | Default |
|------|---------|
| Config directory | `~/.config/hiptty` |
| Session file | `{config_dir}/{profile}.session.json` |
| Profile | `default` (`--profile`) |

On macOS, an existing session under `~/Library/Application Support/hiptty/` is migrated automatically on first run.

## JSON envelope

Success:

```json
{ "schema_version": 1, "ok": true, "data": { } }
```

Failure:

```json
{
  "schema_version": 1,
  "ok": false,
  "error": { "code": "AUTH_REQUIRED", "message": "...", "retryable": false }
}
```

Error codes: `AUTH_REQUIRED`, `AUTH_FAILED`, `NETWORK`, `PARSE`, `RATE_LIMIT`, `FORUM_MESSAGE`, `NOT_IMPLEMENTED`, `INVALID_INPUT`, `NOT_FOUND`.

## Auth

| Command | `data` shape |
|---------|----------------|
| `auth login` | `{ "logged_in": true, "username": "...", "uid": "..." }` |
| `auth logout` | `{ "logged_out": true }` |
| `auth status` | `{ "logged_in": bool, "username": string\|null, "uid": string\|null }` |

## Read commands

| Command | Notes |
|---------|-------|
| `forums list` | Static forum list from `hiptty-core` |
| `threads list --fid N [--page N]` | Thread summaries |
| `thread show TID [--page N] [--at-pid PID] [--last]` | `ThreadDetail` with `Post[]` |
| `search QUERY [--fid N] [--author NAME] [--page N] [--fulltext]` | Search results |
| `my threads\|replies\|favorites\|attention [--page N]` | Requires login |
| `pm list\|new\|show UID\|check` | PM operations |
| `notifications` | Notification list |
| `user show UID` | Profile |
| `blacklist list\|add\|remove` | Blacklist |

### Post (`ThreadDetail.posts[]`)

| Field | Type | Notes |
|-------|------|-------|
| `pid`, `floor`, `author`, `time`, `page` | | |
| `uid`, `avatar_url` | optional | |
| `content` | `ContentNode[]` | Text spans, images, attachments, quotes |
| `poll` | optional | Floor 1 only when thread is a poll |
| `warned` | bool | |
| `signature` | optional string | Parsed from `div.signatures`; reserved for future clients |

### Content

- `ContentNode::Text` contains `spans[]` (`ContentSpan::Text` or `ContentSpan::Smiley`).
- Smiley `code` aligns with hipda (`default_lol`, etc.).
- `Image.size` is bytes when Discuz markup includes `(NNN KB)`.

## Write commands

| Command | Notes |
|---------|-------|
| `post reply-thread TID CONTENT` | Auto-prepares form |
| `post reply-post TID PID CONTENT` | |
| `post quote TID PID CONTENT` | |
| `post new-thread FID SUBJECT CONTENT [--type-id N]` | |
| `post edit TID PID FID PAGE CONTENT [--subject S] [--delete]` | |
| `post delete TID PID FID` | Quick delete via edit form |
| `pm send UID CONTENT` | |
| `pm delete UID` | |
| `favorite add\|remove TID` | |
| `blacklist add\|remove USERNAME` | |

### Inline images in post content

Automatic detection outside existing BBCode tags:

| Input | Result |
|-------|--------|
| Local path to an image file | Upload → `[attachimg]id[/attachimg]` + `attachnew[][description]` |
| `http(s)://...` URL with image extension | `[img]url[/img]` (no download) |

### Post throttling

Reply/new-thread operations enforce a **30 second** interval in the adapter (`RATE_LIMIT` on violation). Edit and quick-delete are not throttled.

## Reserved / not implemented

These exist in the adapter trait for future clients; CLI may be incomplete or stubbed.

| API | Status |
|-----|--------|
| `prepare_post(action)` / `post prepare` | **Reserved** — needs tid/fid/pid CLI args; `post reply` etc. prepare internally |
| `upload_image(action, bytes)` / standalone image upload CLI | **Reserved** — used internally by inline image upload |
| Vote / poll submit | **Not implemented** |
| Text emoticons (`:lol:`) in write path | Pass-through; render mapping deferred to future TUI/assets |
| Smiley GIF rendering | Parse-only; local assets crate deferred |

## Dev

```bash
cargo test --workspace
cargo test -p hiptty-cli -- --ignored   # optional network fixture dump
```