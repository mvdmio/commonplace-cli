# commonplace

`commonplace` is a command-line tool for the [Commonplace](https://commonplace.mvdm.io) REST API. It is built for
people who script Commonplace from a terminal, and for their Agents (Claude Code, Codex, Gemini CLI, and others)
that run shell commands. Every command prints JSON, and the exit code says what happened.

For the full setup steps, read the [CLI guide](https://commonplace.mvdm.io/agents/cli).

The CLI talks to the Commonplace REST API only. It collects no usage data and sends none: no telemetry, no
crash reports, no update checks.

## Install

Linux and macOS:

```sh
curl -LsSf https://commonplace.mvdm.io/cli/install.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://commonplace.mvdm.io/cli/install.ps1 | iex"
```

The installer puts the `commonplace` binary in `~/.cargo/bin` (or `$CARGO_HOME/bin`) and tells you if that folder
is not on your `PATH`. Builds exist for Linux (x64 and arm64, static), macOS (arm64 and x64), and Windows (x64).

Run the install line again to upgrade: the CLI never updates itself. `commonplace --version` shows the build you
run.

## For Agents

```sh
commonplace skill
```

`skill` prints one Markdown skill file that teaches an Agent when and how to use the CLI: signing in, command
names, exit codes, files, our Assistant, and Browser handoffs. It is the only command whose output is not JSON.
Give it to your Agent as a skill, or let the Agent run it. The same file is [`SKILL.md`](SKILL.md) in this repo.

## Sign in

```sh
commonplace login
```

`login` opens your browser at Auth. Sign in there and allow the CLI; the terminal then prints
`{"status":"logged_in","host":…,"user":{"name":…,"email":…},"account":{…}}`. While it waits, stderr holds
`{"status":"browser_opened","authorizeUrl":…}`, so you can open the link by hand. It gives up after 5 minutes.

On a machine without a browser, such as a server over SSH, use the device code. `login` falls back to it by
itself when it cannot open a browser.

```sh
commonplace login --device
```

stderr then holds `{"status":"device_code","verificationUriComplete":…,"verificationUri":…,"userCode":…,"expiresIn":…}`.
Open the link on any device and sign in there. The CLI waits until you have.

The CLI finds Auth by itself, through the host's Protected Resource Metadata. You only ever set the Commonplace
host.

A sign-in is an Agent connection: it acts as you, in one of your Accounts at a time. The CLI renews its access
token by itself. When the connection has ended, commands fail with `not-signed-in`; run `commonplace login` again.

```sh
commonplace logout
```

`logout` ends the Agent connection at Auth, so it stops working everywhere, and then forgets the sign-in. If Auth
cannot be reached, the sign-in stays, and you can run `logout` again.

### Stored sign-in

The sign-in is stored in `credentials.json`, in the `commonplace` folder of your config folder:

| OS | Folder |
| -- | ------ |
| Linux | `$XDG_CONFIG_HOME/commonplace` or `~/.config/commonplace` |
| macOS | `~/Library/Application Support/commonplace` |
| Windows | `%APPDATA%\commonplace` |

`COMMONPLACE_CONFIG_DIR` sets another folder. The file holds one sign-in per host, so a sign-in on another host
sits beside it. On Linux and macOS, only you can read it (mode 0600).

### Personal token

For unattended jobs, such as CI, set `COMMONPLACE_TOKEN` to a Personal token. You create Personal tokens on the API
& Agent Access page of your account; the CLI never creates them. `COMMONPLACE_TOKEN` always wins over the stored
sign-in, and `logout` leaves it alone.

```sh
export COMMONPLACE_TOKEN=...
commonplace api GET /api/v1/books
```

## Status and Accounts

```sh
commonplace status
```

`status` shows who you are and where commands land:
`{"host":…,"signedIn":…,"credential":{"kind":…,"source":…},"user":…,"account":…}`. The credential
kind is `personal-token` or `agent-connection`, and the source is `COMMONPLACE_TOKEN` or `stored sign-in`. `user`
comes from the sign-in, so it is `null` with a Personal token: the API has no call that names the User. Without
a credential it prints `"signedIn": false` and exits 0.

```sh
commonplace accounts list
commonplace accounts switch 42
```

`accounts list` lists your Accounts, the current one marked `"current": true`. `accounts switch <id>` moves the
credential into another Account, for every later command and for everyone who uses that credential.

## Commands from the API

Every REST operation is a command. The CLI builds these commands when it runs, from the host's OpenAPI description
at `/openapi/v1.json`, so a new operation shows up without a new CLI release.

```sh
commonplace books list --top 5
commonplace notes list --filter "contains(name, 'Seneca')"
commonplace books create --title "Letters from a Stoic"
commonplace images get 3f2c9a10-5b1e-4c0a-9d7e-0a1b2c3d4e5f --out cover.png
```

- **Names.** An operation's `operationId` `<group>.<action>` becomes `commonplace <group> <action>`, both in
  kebab-case. So `books.update` is `commonplace books update`. The group comes from the `operationId`, not from the
  tag: `commonplace <group> --help` lists the tags its actions carry.
- **Arguments.** Path parameters are positional arguments, in path order. Query parameters are options, with the
  OData `$` dropped: `--filter`, `--select`, `--orderby`, `--expand`, `--top`, `--skip`. The top-level fields of a
  JSON body are options too, kebab-cased (`notebookId` is `--notebook-id`) and typed from the schema: numbers
  are sent as numbers, `true`/`false` as booleans, a list option repeats, and an object takes JSON text.
- **`--body <json|@file>`** sends a whole body. Field options win over the same keys in it. An operation that
  needs a body and gets none sends `{}`.
- **Name clashes.** `--help`, `--body`, `--out`, and `--file` are always the CLI's own. After them, query
  parameters take their names before body fields. A name already taken gets a `query-` or `field-` prefix (so a
  body field `out` is `--field-out`), and the option's help says so.
- **Downloads.** An operation that answers a file, not JSON, needs `--out <path>`. Without it, the command is a
  usage mistake and sends nothing. Only these operations have `--out`.
- **Help.** `commonplace --help` lists the hand-written commands and the groups. `commonplace <group> --help` lists
  its actions, and `commonplace <group> <action> --help` shows the operation's summary, its description, and every
  argument with its type.

The hand-written commands (`login`, `logout`, `status`, `accounts list`, `accounts switch`, `chat send`, `api`,
`skill`, and `--version`) never need the description, and win over a generated command of the same
name. Other actions in the same group stay reachable. A hand-written command without actions, such as `status`,
hides a generated group of the same name.

### Files

`--file <path>` applies to an Assistant message only: `chat send`, and its generated form
`conversations send-message`. The CLI sends the file first and then makes the call with the new upload id.
`--file` and `--upload-ids` cannot be given together.

```sh
commonplace conversations send-message <conversationId> --text "See these" --file a.pdf --file b.png
```

An image in a note or book is not attached with `--file`. See "Put an image in a note or book" in
[`SKILL.md`](SKILL.md).

- **Lists.** `--file` repeats. Each file becomes one id in `uploadIds`, in order. A file that cannot be read fails
  with `file` before any file is sent.
- **Parts.** Each file goes through its own Upload link. A file up to the link's part size goes in one request; a
  larger one goes in parts of that size and then a finish. The part size always comes
  from the link. A file larger than the link takes fails with `file` before any bytes go.
- **Resume.** When the link answers 409 with the number of bytes it holds, the CLI goes on from there. When a
  request gets no usable answer, the CLI asks the link how many bytes it holds and goes on from there. A file
  whose one request failed goes on in parts, since the link takes a whole file only while it holds no byte.
  After five tries in a row that move no byte forward, it fails with `upload`.
- **The credential stays home.** The Upload link needs no bearer, so the CLI sends none to it.
- stdout holds only the result of the final call. The CLI prints no upload progress.

### The cached description

The CLI keeps a copy of the description per host, in the `commonplace` folder of your cache folder:

| OS | Folder |
| -- | ------ |
| Linux | `$XDG_CACHE_HOME/commonplace` or `~/.cache/commonplace` |
| macOS | `~/Library/Caches/commonplace` |
| Windows | `%LOCALAPPDATA%\commonplace` |

`COMMONPLACE_CACHE_DIR` sets another folder. A copy younger than one hour is used as it is. An older one is
fetched again; if that fails, the old copy is used. With no copy and no answer from the host, a generated command
fails with `network`.

A new operation reaches you by itself. When you type a group or action the copy does not know, the CLI fetches a
fresh copy at once, even within the hour. If the command is still unknown, it is a usage mistake (exit 2).

## Talk to our Assistant

```sh
commonplace chat send "What did I note about Seneca?"
```

`chat send "<text>"` sends one message to our Assistant, as a person does in the Assistant panel, and waits
until its turn ends. Without `--conversation <id>`, it starts a new conversation. It then prints:

```json
{"conversationId":"…","lastError":null,"queued":false,"messages":[…]}
```

- `messages` holds every message after the one you sent, oldest first, as the API gives it: `seq`, `role`,
  `sender`, the markdown `text`, `attachments`, and `toolActivity` (the tools our Assistant ran, each with its
  outcome).
- `conversationId` is the conversation your message landed in. When our Assistant judges it a new topic, this is
  a new conversation, not the one you named. Continue with that id.
- `lastError` says why the turn stopped early, when it did.
- `queued` is true when your message went into a shared conversation while another member's turn was running.
  Our Assistant answers it in the turn after that one, so `messages` may hold only that member's reply. Read the
  answer later with `commonplace conversations list-messages <conversationId> --after <seq> --wait 60`.

**Attach files.** `--file <path>` attaches a file to the message, and repeats for more files. The CLI uploads each
file to the `conversation` upload target first, before any conversation starts, and sends the upload ids with the
message. A file that cannot be read fails with `file` and sends nothing.

```sh
commonplace chat send "Describe this" --file photo.jpg
```

**Answer a question or proposal.** Our Assistant asks and proposes in the text of its messages. Answer with the
next `chat send`, passing the printed `conversationId`:

```sh
commonplace chat send "Yes, go ahead." --conversation <conversationId>
```

An empty Assistant allowance (402 `allowance-exhausted`) or a conversation that cannot take a message now (409)
prints the ProblemDetails on stderr and exits 1. For everything else about conversations, such as listing them or
stopping a turn, use the generated `commonplace conversations …` commands.

## Raw calls

`commonplace api <method> <path>` makes one call with your credential. The path is relative to the host, and a
query string rides in it.

```sh
commonplace api GET '/api/v1/books?$top=5'
commonplace api POST /api/v1/books --body '{"title":"Letters from a Stoic"}'
commonplace api PATCH /api/v1/books/12 --body @book.json
commonplace api GET /api/v1/images/3f2c9a10-5b1e-4c0a-9d7e-0a1b2c3d4e5f --out cover.png
```

- `--body <json|@file>` sends a JSON body, inline or read from a file.
- `--out <path>` writes the response body to a file and prints `{"path","bytes","contentType","fileName"}`. A
  response that is not JSON needs `--out`: the CLI never writes raw bytes to stdout.

## Output

- stdout holds the JSON result: pretty-printed in a terminal, compact otherwise. An empty success prints
  `{"ok":true,"status":<code>}`.
- stderr holds errors: the API's ProblemDetails as it came, or `{"error":"<code>","message":"…"}` for a failure
  on your side, such as `not-signed-in`, `network`, `file`, `upload`, or `binary-response`.
- On a rate limit (429) the CLI waits for `Retry-After` and retries, up to three times.

## Exit codes

| Code | Meaning |
| ---- | ------- |
| 0 | Success. |
| 1 | Error: see stderr. |
| 2 | Usage mistake: fix the command line. |
| 3 | Browser handoff: a person must finish the act in the browser. |

A Browser handoff prints `{"status":"browser_handoff","reason":…,"handoffUrl":…}` on stdout. Show the link to a
person; the CLI never opens a browser for it.

## Environment

| Variable | Meaning |
| -------- | ------- |
| `COMMONPLACE_TOKEN` | A Personal token, sent as the bearer. Wins over the stored sign-in. |
| `COMMONPLACE_URL` | The Commonplace host. Defaults to `https://commonplace.mvdm.io`. |
| `COMMONPLACE_CONFIG_DIR` | The folder for the stored sign-in. |
| `COMMONPLACE_CACHE_DIR` | The folder for the cached API description. |

## Develop

The source is the [`commonplace-cli`](https://github.com/mvdmio/commonplace-cli) repository: a Rust crate whose binary
is `commonplace`. It needs the stable Rust toolchain.

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The tests run the built binary against fake Commonplace and Auth hosts started inside the test process, so they need
no network, no browser, and no account. [`AGENTS.md`](AGENTS.md) describes the layout and the release steps, and
[`CONTEXT.md`](CONTEXT.md) the terms.

## Licence

MIT. See [LICENSE](LICENSE).
