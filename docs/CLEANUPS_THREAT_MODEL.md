# User-defined cleanups: what they may do, and what they may not

WinDirStat calls them "Cleanups": commands a user configures once and
then runs against whatever is selected in the tree. They are genuinely
useful — "open a shell here", "compress this folder", "send this to my
own script" — and they are also the single most dangerous thing an app
like this can grow, because the app's other buttons already delete files
and this one runs arbitrary programs with the user's privileges against a
path the user pointed at.

The roadmap deferred them with "wants a threat model written before any
code". This is that document; the rules at the bottom are what the
implementation in `src/cleanups.rs` actually enforces, and every one of
them has a test.

## What the feature is

A cleanup is a name, a program, and a list of arguments. Placeholders in
the arguments are replaced with facts about the selected item before the
program is launched. Cleanups live in the user's config file and are
edited there; the app runs them and reports what happened.

## Who the attacker is

Three of them, in descending order of likelihood.

1. **The filesystem.** File and directory names are attacker-controlled
   in a way people forget: anyone who can write into a scanned directory
   chooses the *text* this feature substitutes. A file called
   `; rm -rf ~`, or `--upload-to=evil.example`, or one whose name is
   nothing but a quote character, arrives here as a name to substitute.
   This is the realistic threat and it needs no attacker on the machine
   at all — an unzipped archive is enough.
2. **The config file.** Anything that can write `config.toml` can name a
   program to run. That is already true of anything that can write the
   user's home directory, so the config is not a privilege boundary; what
   matters is that reading it can never be *surprising*, i.e. the app
   must not synthesise commands the user did not write.
3. **The user, misunderstanding.** Someone who believes a cleanup will be
   confirmed, or will not recurse, or will act on the selection rather
   than its parent, and is wrong.

## What could go wrong, and what stops it

| Threat | What stops it |
|---|---|
| A filename is interpreted as shell syntax (`; rm -rf ~`, backticks, `&&`) | **No shell, ever.** The program is launched with an argv array through `std::process::Command`; nothing is ever handed to `sh -c` or `cmd /c`, and the app never splits a string into arguments. A name containing a semicolon is one argument that contains a semicolon. |
| A filename is interpreted as an *option* by the program (`--delete-everything`) | Arguments are configured as a list, so a substituted value is always exactly one argv element and never becomes a new one. The app cannot stop `rm` from believing a file called `-rf` is a flag, so the config format supports `--` as a literal argument and the docs say to use it. |
| A cleanup runs against the wrong item after a rescan | The path is resolved **exactly** (`Tree::path_for`), never through the forgiving `deepest_valid_path`. A stale selection refuses rather than acting on the nearest surviving ancestor — the same rule the delete path already follows. |
| A cleanup runs with nothing selected, and acts on the scan root | Refused. There is no implicit target. |
| A destructive cleanup runs without the user meaning it | Every cleanup is confirmed by default; a cleanup may opt *out* (`confirm = false`) but must say so in its own config entry. |
| A cleanup is triggered by something other than the user | Cleanups have no keyboard shortcut, no toolbar button, and no default entries. An app that shipped with cleanups pre-configured would be shipping commands nobody read. |
| The app is turned into a launcher for arbitrary code by a malicious config | Not preventable and not this app's boundary to hold: a config file that can name a program is equivalent to a shell profile that can. What *is* prevented is the app inventing a command from data — nothing outside the config can name a program. |
| A cleanup hangs and takes the window with it | It runs on a worker thread through the same channel the Windows maintenance tools use, and the window stays interactive. |
| Output leaks somewhere unexpected | Captured and shown in the app's own tool log. Nothing is written to a file the user did not name. |

## The rules

1. **No shell.** Never `sh -c`, never `cmd /c`, never string-splitting.
   A user who wants a shell names one as the program, in their own
   config, where they can see it.
2. **Arguments are a list, not a string.** Substitution happens *inside*
   one argument and never creates another.
3. **Placeholders are explicit**: `%p` full path, `%n` file name, `%d`
   parent directory, `%%` a literal `%`. An unknown placeholder is an
   error, not a silent empty string — `%q` becoming nothing is how a
   command quietly runs against the wrong thing.
4. **Exact resolution.** The selection resolves through `path_for`; a
   stale one refuses.
5. **Confirm by default.** Opting out is per-cleanup and explicit.
6. **No defaults.** The app ships with no cleanups configured.
7. **The command is shown before it runs.** The confirmation names the
   program and every argument after substitution, so what is about to
   happen is readable rather than inferred from a template.

## What is deliberately not done

- **No `%s` "selection list" placeholder.** A cleanup runs against one
  item. Multi-selection would mean deciding whether a command is run once
  per item or once with many arguments, and getting that wrong is the
  difference between deleting one folder and deleting twenty.
- **No environment-variable expansion by the app.** The OS expands what
  it expands when the program starts; a second layer of expansion in the
  app would be one more place a filename can be reinterpreted.
- **No working-directory default of "the selected folder".** It would be
  a second, invisible way the selection reaches the command. A cleanup
  that wants it passes `%d` or `%p` explicitly.
