# subnet-config — Code Explainer

A line-by-line walkthrough of the shell idioms, patterns, and design choices
in `subnet-config`. Organized by section of the script.

---

## Table of Contents

- [Shebang and set -e](#shebang-and-set--e)
- [Finding the script's own directory](#finding-the-scripts-own-directory)
- [Variables and scope](#variables-and-scope)
- [Functions](#functions)
- [The INI parser](#the-ini-parser)
- [eval and security](#eval-and-security)
- [Pattern matching with case](#pattern-matching-with-case)
- [The callback pattern (for_each_output)](#the-callback-pattern-for_each_output)
- [Grouped output with braces](#grouped-output-with-braces)
- [Word splitting (the unquoted for loop)](#word-splitting-the-unquoted-for-loop)
- [Test expressions](#test-expressions)
- [Short-circuit idioms](#short-circuit-idioms)
- [Pipes and subshells](#pipes-and-subshells)
- [Argument parsing with getopts](#argument-parsing-with-getopts)
- [Output and redirection](#output-and-redirection)
- [Exit codes and error handling](#exit-codes-and-error-handling)

---

## Shebang and set -e

```sh
#!/bin/sh
set -e
```

`#!/bin/sh` tells the OS to run this script with `/bin/sh`. We use `/bin/sh`
rather than `/bin/bash` because we want the script to work on any Unix-like
system. The script avoids "bashisms" — bash-only syntax like `[[ ]]`, arrays,
or `source` — so it runs under bash, zsh, dash, and other POSIX-ish shells.

`set -e` makes the script exit immediately if any command fails (returns a
non-zero exit code). Without this, the script would continue past errors
silently. It's a safety net — if something goes wrong, we stop rather than
producing corrupt output.

One subtlety: commands in `if` conditions, `||` chains, and `&&` chains are
exempt from `set -e`. That's why `diff ... || true` on line 214 doesn't kill
the script — the `|| true` makes it part of a chain.

---

## Finding the script's own directory

```sh
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
```

This is a common shell idiom to get the absolute path to the directory
containing the script, no matter where it was invoked from. Breaking it apart:

| Part | Meaning |
|------|---------|
| `$0` | The path used to invoke the script (e.g. `./subnet-config` or `/home/me/bin/subnet-config`) |
| `dirname "$0"` | Strip the filename, leaving just the directory (e.g. `.` or `/home/me/bin`) |
| `$(...)` | Command substitution — run the command inside and replace with its stdout |
| `cd "..." && pwd` | Change into that directory, then print the absolute path |

The outer `$(...)` captures the result of `pwd` into `SCRIPT_DIR`. This gives
us an absolute path even if the script was invoked with a relative path like
`../subnet-config`.

We use this to locate the default config file (`$SCRIPT_DIR/subnet.conf`) —
it lives next to the script regardless of the user's working directory.

---

## Variables and scope

```sh
# Global — set at top level
authority_interface=""
radvd_adv_autonomous="true"
HAVE_DIFFS=0

# Local — inside a function
local config="$1"
local section="" line key value
```

Shell variables are **global by default**. Any variable you assign inside a
function is visible everywhere else in the script. `local` restricts a
variable's lifetime to the function that declares it.

Technically `local` isn't part of the POSIX sh standard, but every real-world
`/bin/sh` supports it (bash, dash, zsh, ash, mksh, busybox sh). The only
shell that doesn't is the original 1970s Bourne shell, which no one runs
anymore.

The config variables (`authority_interface`, `radvd_adv_managed`, etc.) are
deliberately global — the INI parser sets them, and the generators read them.
`HAVE_DIFFS` is also deliberately global — `check_file` sets it from inside
`for_each_output`, and `cmd_check` reads it afterward. This works because
`for_each_output` calls `check_file` directly (not through a pipe), so
everything executes in the same shell process.

---

## Functions

```sh
error() {
    printf "Error: %s\n" "$1" >&2
    exit 1
}
```

Functions in sh are defined with `name() { body; }`. There's no `function`
keyword (that's a bashism). Parameters are accessed as `$1`, `$2`, etc. —
there's no named-parameter syntax.

Functions share the calling shell's environment. They can read and write
global variables, and `exit` inside a function exits the *entire script*, not
just the function. To exit only the function, you'd use `return`.

---

## The INI parser

```sh
while IFS= read -r line || [ -n "$line" ]; do
    ...
done < "$config"
```

This is the standard idiom for reading a file line by line in POSIX sh.

| Part | Purpose |
|------|---------|
| `IFS=` | Temporarily clear the Internal Field Separator so leading/trailing whitespace isn't stripped by `read` |
| `read -r line` | Read one line into variable `line`. `-r` prevents backslash interpretation (so `\n` stays as literal `\n`) |
| `\|\| [ -n "$line" ]` | Handle files that don't end with a newline. `read` returns false at EOF, but if there's a partial last line, `$line` will be non-empty and we still want to process it |
| `done < "$config"` | Redirect the file into the while loop's stdin |

The whitespace trimming is then done explicitly with sed:

```sh
line="$(printf '%s' "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
```

This pipes the line through `sed` with two substitutions separated by `;`:
- `s/^[[:space:]]*//` — delete leading whitespace
- `s/[[:space:]]*$//` — delete trailing whitespace

`[[:space:]]` is a POSIX character class matching spaces, tabs, etc. It's
more portable than `\s` or literal space/tab characters.

### Parsing sections

```sh
if printf '%s' "$line" | grep -q '^\[.*\]$'; then
    section="$(printf '%s' "$line" | sed 's/^\[\(.*\)\]$/\1/')"
```

`grep -q` tests whether the line matches the pattern without printing
anything (`-q` = quiet). The pattern `^\[.*\]$` matches lines that start with
`[` and end with `]` — section headers like `[authority]`.

The `sed` command then extracts just the section name. `\(` and `\)` create a
capture group, and `\1` refers to what it captured. So `\[\(.*\)\]` applied
to `[authority]` captures `authority`.

### Parsing key = value

```sh
key="$(printf '%s' "$line" | sed 's/[[:space:]]*=.*//')"
value="$(printf '%s' "$line" | sed 's/^[^=]*=[[:space:]]*//' | sed 's/[[:space:]]*#.*$//' | sed 's/[[:space:]]*$//')"
```

For a line like `interface = eth0 wlan0          # Space-separated list`:

| Step | sed command | Result |
|------|------------|--------|
| Extract key | `s/[[:space:]]*=.*//'` | `interface` (delete from whitespace-before-`=` onward) |
| Extract value step 1 | `s/^[^=]*=[[:space:]]*//'` | `eth0 wlan0          # Space-separated list` (delete up through `= `) |
| Strip inline comment | `s/[[:space:]]*#.*$//'` | `eth0 wlan0` (delete from whitespace-before-`#` onward) |
| Strip trailing space | `s/[[:space:]]*$//'` | `eth0 wlan0` (clean up) |

The three `sed` calls are piped together. Each one transforms the text and
passes it to the next.

---

## eval and security

```sh
case "$key" in
    *[!a-zA-Z0-9_]*) error "Invalid config key: $key" ;;
esac

case "$section" in
    authority) eval "authority_$key=\"\$value\"" ;;
```

`eval` takes a string and executes it as shell code. Here it's used to set a
dynamically-named variable. If `section` is `authority` and `key` is
`interface`, the eval runs:

```sh
authority_interface="$value"
```

The `\$value` is important — the backslash delays expansion. Without it, the
value would be expanded *before* eval processes the string, which would break
on values containing spaces or special characters. With the backslash, `eval`
sees the literal `$value` and expands it itself, properly quoted.

`eval` is dangerous because it executes arbitrary code. If a config file
contained a key like `x$(rm -rf /)`, the eval would run that command. The
`case` guard above prevents this by rejecting any key that contains characters
outside `a-zA-Z0-9_`. The pattern `*[!a-zA-Z0-9_]*` means "contains at least
one character that is NOT a letter, digit, or underscore." The `!` inside
`[...]` means negation in POSIX sh (bash uses `^`, but `!` is the portable
form).

---

## Pattern matching with case

```sh
case "$line" in
    ''|'#'*) continue ;;
esac
```

`case` is the shell's pattern-matching construct (like a switch statement).
Each pattern uses glob syntax, not regex:

| Pattern | Matches |
|---------|---------|
| `''` | Empty string |
| `'#'*` | Anything starting with `#` |
| `true\|yes\|1` | Literal `true`, `yes`, or `1` |
| `*[!a-zA-Z0-9_]*` | String containing a non-alphanumeric, non-underscore char |

`|` separates alternatives. `;;` terminates each branch (like `break` in C).
`continue` skips to the next iteration of the enclosing loop.

The `normalize_bool` function is a good example of `case` replacing what would
be an if/elif chain:

```sh
normalize_bool() {
    case "$2" in
        true|yes|1) eval "$1=true" ;;
        false|no|0) eval "$1=false" ;;
        *) error "Invalid boolean for $1: $2" ;;
    esac
}
```

Called as `normalize_bool radvd_adv_managed "$radvd_adv_managed"` — the first
argument is the variable *name* (not its value), the second is the current
value. The eval sets the variable by name.

---

## The callback pattern (for_each_output)

```sh
for_each_output() {
    local callback="$1"
    local iface

    "$callback" "radvd.conf" "/etc/radvd.conf"
    for iface in $authority_interface; do
        "$callback" "50-subnet-authority-$iface.network" \
            "/etc/systemd/network/50-subnet-authority-$iface.network"
    done
    "$callback" "subnet-authority.conf" "/etc/dnsmasq.d/subnet-authority.conf"
}
```

This is a poor-man's higher-order function. POSIX sh doesn't have first-class
functions or arrays, but you can pass a function *name* as a string and call
it with `"$callback" args...`. This centralizes the file mapping in one place
— `cmd_check` calls `for_each_output check_file` and `cmd_copy` calls
`for_each_output copy_file`, and neither needs to know the list of files.

Since `"$callback"` is called directly (not through a pipe), it runs in the
same shell process. That's why `check_file` can set the global `HAVE_DIFFS`
variable and `cmd_check` can read it afterward.

The `\` at the end of a line is a line continuation — the next line is treated
as part of the same command. It's just for readability.

---

## Grouped output with braces

```sh
{
    printf "# Generated by subnet-config — do not edit manually\n\n"
    printf "interface %s {\n" "$iface"
    ...
} > "$output"
```

The `{ ... }` groups multiple commands so they share a single redirection.
Without this, each `printf` would need its own `>> "$output"` (append), and
the first one would need `>` (truncate). The brace group is cleaner: all the
printfs write to stdout, and the single `>` redirects that collective stdout
to the file.

This is different from `( ... )` which runs commands in a *subshell* (a child
process with its own variable scope). `{ ... }` runs in the current shell —
cheaper and variables set inside are visible outside.

---

## Word splitting (the unquoted for loop)

```sh
for iface in $authority_interface; do
```

Note: `$authority_interface` is **intentionally unquoted**. When the shell
expands an unquoted variable, it splits the result on whitespace (spaces,
tabs, newlines). So if `authority_interface="eth0 wlan0"`, the loop iterates
twice: once with `iface=eth0`, once with `iface=wlan0`.

This is normally a bug — unquoted variables are one of the most common shell
scripting mistakes. But here it's the mechanism for iterating over a
space-separated list, since POSIX sh has no arrays. The same pattern appears
with `$dns_upstream`.

By contrast, `"$authority_interface"` (quoted) would be treated as a single
string, and the loop would run once with `iface="eth0 wlan0"`. Always quote
variables unless you specifically want word splitting.

---

## Test expressions

```sh
[ -f "$config" ]          # true if $config is a regular file
[ -d "$OUTPUT_DIR" ]      # true if $OUTPUT_DIR is a directory
[ -n "$line" ]            # true if $line is non-empty (-n = non-zero length)
[ ! -f "$generated" ]     # true if $generated does NOT exist as a file
[ "$HAVE_DIFFS" -eq 0 ]   # integer equality
[ "$radvd_rdnss" = "true" ]  # string equality
```

`[` is actually a command (often a shell builtin, also available as
`/usr/bin/[`). It evaluates a conditional expression and returns 0 (true) or
1 (false). The `]` is a required closing argument.

This is different from `[[ ]]` which is a bash/zsh keyword with more features
(regex matching, no word splitting). We use `[ ]` for POSIX compatibility.

The operators:

| Operator | Type | Meaning |
|----------|------|---------|
| `-f` | File test | Is a regular file |
| `-d` | File test | Is a directory |
| `-n` | String test | Is non-empty |
| `!` | Negation | Invert the test |
| `=` | String compare | Strings are equal |
| `-eq` | Integer compare | Numbers are equal |

---

## Short-circuit idioms

```sh
[ -f "$config" ] || error "Config file not found: $config"
```

`||` means "if the left side fails (returns non-zero), run the right side."
This is equivalent to:

```sh
if [ ! -f "$config" ]; then
    error "Config file not found: $config"
fi
```

Similarly, `&&` means "if the left side succeeds, run the right side":

```sh
[ $# -eq 0 ] && usage
```

Means: "if there are zero arguments, call usage." These short-circuit forms
are idiomatic in shell scripts for simple conditional one-liners. For anything
more complex, an `if` block is clearer.

One thing to know: `set -e` does **not** trigger on the left side of `||` or
`&&`. So `[ -f "$config" ] || error ...` won't abort the script if the test
fails — it will execute the `error` function as intended.

---

## Pipes and subshells

```sh
printf '%s' "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'
```

`|` (pipe) connects the stdout of the left command to the stdin of the right.
`printf` writes the line, `sed` reads it and applies the substitution.

Each side of a pipe runs in a separate subshell (child process). This is
important for variable assignment — if you set a variable inside a piped
command, the change is invisible to the parent:

```sh
# This does NOT work as expected:
echo "hello" | read word    # $word is empty afterward in most shells
```

That's why `while ... done < "$config"` uses file redirection (`<`) instead
of `cat "$config" | while ...`. The redirect version runs the loop in the
current shell, so variables set inside (like `section`, `key`, `value`) are
visible after the loop ends.

Command substitution `$(...)` also runs in a subshell, but it captures stdout
as a string:

```sh
key="$(printf '%s' "$line" | sed 's/[[:space:]]*=.*//')"
```

The subshell runs the pipeline, and its stdout becomes the value of `$key`.

---

## Argument parsing with getopts

```sh
while getopts "c:o:h" opt; do
    case "$opt" in
        c) CONFIG_FILE="$OPTARG" ;;
        o) OUTPUT_DIR="$OPTARG" ;;
        h) usage ;;
        *) usage ;;
    esac
done

shift $((OPTIND - 1))
```

`getopts` is a POSIX builtin for parsing command-line flags. The string
`"c:o:h"` defines the accepted options:

| Char | Meaning |
|------|---------|
| `c:` | `-c` takes an argument (the `:` means "requires a value") |
| `o:` | `-o` takes an argument |
| `h` | `-h` is a flag (no argument) |

On each iteration, `getopts` sets `$opt` to the option letter and `$OPTARG`
to its argument (if any). When all options are consumed, `getopts` returns
false and the loop exits.

`$OPTIND` tracks how many arguments getopts consumed. `shift $((OPTIND - 1))`
removes the processed options from `$@`, leaving only the positional
arguments (the subcommand). After the shift, `$1` is the subcommand
(`generate`, `check`, etc.).

`$((...))` is arithmetic expansion — it evaluates the math expression inside
and substitutes the result. `$((OPTIND - 1))` subtracts 1 from OPTIND.

---

## Output and redirection

```sh
printf "Error: %s\n" "$1" >&2
```

`printf` is preferred over `echo` in portable scripts because `echo` behaves
differently across shells (some interpret `\n`, some don't, some accept `-n`,
some don't). `printf` is consistent everywhere.

`printf` syntax: the first argument is a format string (like C's printf), the
rest fill in the `%s` (string), `%d` (integer), etc. placeholders. `\n` is a
literal newline in the format string.

`>&2` redirects stdout to stderr (file descriptor 2). Error messages go to
stderr so they don't contaminate stdout, which might be piped to another
program.

Other redirections used in the script:

| Syntax | Meaning |
|--------|---------|
| `> "$output"` | Write stdout to file (truncate) |
| `> /dev/null 2>&1` | Discard both stdout and stderr |
| `< "$config"` | Read file into stdin |
| `\|\| true` | Ignore the exit code of the preceding command |

`> /dev/null 2>&1` appears in `diff -u ... > /dev/null 2>&1` — we run diff
once silently to check if files differ (using its exit code), then run it
again to actually show the diff output.

---

## Exit codes and error handling

Every command returns an exit code: 0 means success, anything else means
failure. `set -e` makes the script abort on any non-zero exit code (with the
exceptions noted earlier: `if`, `||`, `&&`).

```sh
systemctl restart radvd || error "Failed to restart radvd"
```

If `systemctl` fails (non-zero exit), the `||` triggers and `error` runs
(which prints a message and calls `exit 1`). The `||` also exempts this line
from `set -e`, so both outcomes are handled explicitly.

```sh
diff -u "$system" "$generated" || true
```

`diff` returns 1 when files differ — which is normal, not an error. `|| true`
swallows that exit code so `set -e` doesn't kill the script. The diff output
still prints normally; only the exit code is suppressed.

`return 1` in `cmd_check` causes the function to return a failure code, which
propagates through `case` as the script's exit code. This lets callers check
whether differences were found: `subnet-config check && echo "all good"`.
