# Tilde expansion

In **tilde expansion**, the shell replaces a tilde (`~`) at the start of a word with the value of the `HOME` variable, allowing you to specify paths relative to your home directory.
For example, if `HOME` is `/home/alice`, `~/Documents` expands to `/home/alice/Documents`.

```shell
$ echo ~
/home/alice
$ echo ~/Documents
/home/alice/Documents
```

The `HOME` variable is usually passed as an environment variable to the shell when the user logs in, so you don't need to set it manually.

You can also use `~` followed by a username to refer to another user's home directory:

```shell
$ echo ~bob
/home/bob
$ echo ~bob/Documents
/home/bob/Documents
```

In variable assignments, tilde expansion happens at the start of the value and after each `:` character:

```shell
$ PATH=~/bin:~bob/bin:~clara/bin:/usr/bin
$ echo "$PATH"
/home/alice/bin:/home/bob/bin:/home/clara/bin:/usr/bin
```

If tilde expansion produces a pathname ending with `/` followed by another `/`, one `/` is removed:

```shell
$ HOME=/
$ echo ~/tmp
/tmp
```

In older shells, `//tmp` may be produced instead of `/tmp`, which can refer to a different location. POSIX.1-2024 now requires the behavior shown above.

Tilde expansion only happens at the start of a word, or after each `/` (or `:` in variable assignments). If any part of the expansion or delimiter is quoted, the shell treats them literally:

```shell
$ echo ~'b'ob
~bob
$ echo ~\/
~/
```

Currently, the shell ignores any errors during tilde expansion and leaves the tilde as is. This behavior may change in the future.

The shell may support other forms of tilde expansion in the future, e.g., `~+` for the current working directory.
