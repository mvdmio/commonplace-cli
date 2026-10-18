---
name: commonplace
description: Use the `commonplace` command-line tool to read and change a person's Commonplace journal (books, quotes, notes, reminders) through its REST API, and to talk to its Assistant. Use it when a person asks you to look something up in Commonplace, change it, attach an image to it, or ask its Assistant. Every command prints JSON.
---

# commonplace

`commonplace` calls the Commonplace REST API at `https://commonplace.mvdm.io` for the person you work for. It acts
as that person, in one of their Accounts at a time. Every command prints JSON on stdout, and the exit code says
what happened.

Use it when the person asks about or wants to change anything they keep in Commonplace. Prefer a command over raw
HTTP: the CLI signs in, builds the URL and body, and retries on a rate limit.

Install, or upgrade, with:

```sh
curl -LsSf https://commonplace.mvdm.io/cli/install.sh | sh
```

On Windows: `powershell -ExecutionPolicy ByPass -c "irm https://commonplace.mvdm.io/cli/install.ps1 | iex"`.

## Sign in

Run `commonplace status` first. It prints `"signedIn": true` or `false`, the credential in use, and the Account
commands act in; `user` is `null` with a Personal token. Without a credential it exits 0. With one that no longer
works it exits 1.

- `commonplace login` opens the person's browser at Auth. The person signs in and allows the CLI. You wait; the
  command ends when they have.
- `commonplace login --device` prints a link with the code filled in, and the code, on stderr. Show both to the
  person; they finish on any device. `login` falls back to this by itself when it cannot open a browser, such
  as over SSH.
- `COMMONPLACE_TOKEN` holds a Personal token for unattended runs, such as CI. It wins over the stored sign-in.
  Only the person can create one, on the API & Agent Access page of their account. Never ask them to paste one
  into the chat; ask them to set the variable.
- `COMMONPLACE_URL` points the CLI at another Commonplace host. Leave it unset for `https://commonplace.mvdm.io`.
- `commonplace logout` ends this CLI's stored sign-in at Auth and forgets it; `COMMONPLACE_TOKEN` stays. Do not run
  it unless the person asks.

A command that fails with `not-signed-in` needs `commonplace login`.

## Accounts

- `commonplace accounts list` lists the person's Accounts. The one commands act in has `"current": true`.
- `commonplace accounts switch <id>` moves the credential into another Account, for every later command. Switch
  only when the person asks for another Account, and say which one you switched to.

## Commands

Every REST operation is a command: `commonplace <group> <action>`, both in kebab-case, from its `operationId`
`<group>.<action>`. The operation `books.update` is `commonplace books update <id>`. The group comes from the
`operationId`, not the tag: `legal-acceptance.accept` is `commonplace legal-acceptance accept`.

- `commonplace --help` lists the groups. `commonplace <group> --help` lists its actions.
  `commonplace <group> <action> --help` shows what the operation does and every argument with its type. Read it
  before your first call of an action.
- Path parameters are positional arguments, in path order.
- Query parameters are options. List actions take the OData options `--filter`, `--select`, `--orderby`,
  `--expand`, `--top`, and `--skip`, as in `commonplace books list --top 5` and
  `commonplace notes list --filter "contains(name, 'Seneca')"`.
- Top-level body fields are options, kebab-cased: `notebookId` is `--notebook-id`. A list option repeats.
- `--body '<json>'` or `--body @file.json` sends a whole body, for nested shapes. Field options win over the same
  keys in it.
- `commonplace api <method> <path> [--body …] [--out …]` makes one raw call, for anything the commands do not
  shape well: `commonplace api GET '/api/v1/books?$top=5'`.

A group or action the CLI does not know makes it fetch the API's description again, so a new operation works at
once. A command that is still unknown is a usage mistake.

## Output and exit codes

| Code | Meaning | What to do |
| ---- | ------- | ---------- |
| 0 | Success. stdout holds the JSON result. | Read the result. |
| 1 | Error. stderr holds the API's ProblemDetails, or `{"error":"<code>","message":…}`. | Read `title`, `detail`, or `message`, fix the cause or tell the person. |
| 2 | Usage mistake. | Fix your command line; check `--help`. |
| 3 | Browser handoff. | See below. |

On a rate limit (429) the CLI waits and retries by itself, up to three times. Do not retry an exit 1 in a loop.

## Browser handoff

Accepting the Legal terms is the person's act, in the browser. While the Account has not accepted them, a command
exits 3 and prints:

```json
{"status":"browser_handoff","reason":"…","handoffUrl":"https://…"}
```

Show the `handoffUrl` to the person and tell them the `reason`. The person opens the link and accepts the terms
there. Never open the link yourself, and never try to accept the terms another way. Once the person says they are
done, run the command again.

## Files

- **Send a file to the Assistant.** `--file <path>` applies to an Assistant message only: `chat send` and
  `conversations send-message`. Repeat it for more files. The CLI sends each file first and then the message with the
  new upload ids.

  ```sh
  commonplace chat send "Describe this" --file photo.jpg
  ```

- **Get an image out.** `commonplace images get <key> --out <path>` writes the image there and prints
  `{"path","bytes","contentType","fileName"}`. It never writes file bytes to stdout. `<key>` is the id in an image
  address in a note or book, as in `/blobs/file/{key}/…`.

  ```sh
  commonplace images get 3f2c9a10-5b1e-4c0a-9d7e-0a1b2c3d4e5f --out cover.png
  ```

- **Put an image in a note or book.** A note or book names an image as `upload:<id>`, which you write yourself.
  `commonplace uploads create --target image --file-name cover.png` prints an Upload link with `uploadId` and
  `url`. Send the bytes with one `PUT` to that `url`, then use `upload:<uploadId>` as the cover address or in an
  `<img src="upload:<uploadId>">` in the body. Each upload is used once.

## Talk to the Assistant

Commonplace has its own Assistant, which knows the Account and can act in it. Ask it when the person wants its
judgement, such as a summary or a proposal, rather than one record.

```sh
commonplace chat send "What did I note about Seneca?"
```

It waits until the Assistant's turn ends and prints
`{"conversationId","lastError","queued","messages":[…]}`. Each message has `role`, `sender`, the
markdown `text`, and `toolActivity`.

- **Continue** with `--conversation <conversationId>`, always the id from the last output: it changes when the
  Assistant moves the message to a new topic.
- **Questions and proposals** from the Assistant are in the text of its last message. Put them to the person, or
  answer them yourself when the person already told you what they want, with the next `chat send` on the same
  conversation: `commonplace chat send "Yes, go ahead." --conversation <conversationId>`.
- **`queued: true`** means another member's turn ran first, and `messages` may not hold your answer yet. Read it
  later with `commonplace conversations list-messages <conversationId> --after <seq> --wait 60`, where `<seq>` is
  the last message you saw.
- **`lastError`** says why a turn stopped early. An exit 1 with `allowance-exhausted` means the Account's
  Assistant allowance is used up; tell the person.

## Privacy

The CLI talks only to the Commonplace host and its Auth. It sends no usage data.
