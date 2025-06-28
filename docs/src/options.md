# Shell options

**Shell options** are settings that control the behavior of the shell. They can be set (enabled) or unset (disabled) using command line arguments on startup, or using the `set` built-in during a shell session.

## Enabling and disabling options

Shell options are specified as command line arguments when starting the shell or calling the `set` built-in. In yash, all shell options have a long name, and some have a short name as well.

When options are specified on shell startup, they are set before the shell starts reading and executing commands. When options are specified to the `set` built-in, they are set in the current shell session. Some options can only be set at startup, while others can be set or unset at any time. The syntax for specifying options is the same in both cases.

### Long option names

Long options are specified as an argument starting with `--`. For example, to start yash-rs with the `allexport` option enabled:

```sh
yash3 --allexport
```

Long options can also be specified as an argument to the `-o` option:

```sh
yash3 -o allexport
```

Only alphanumeric characters are significant in long option names, and long options are case-insensitive. For example, `--all-export`, `--ALLEXPORT`, and `---All*Ex!PorT` all enable the `allexport` option.

There are several ways to disable an option with a long name. One way is to prepend `no` to the option name:

```sh
yash3 --noallexport
```

Another is to start an argument with `++` instead of `--`:

```sh
yash3 ++allexport
```

You can also use the `+o` option to disable an option:

```sh
yash3 +o allexport
```

If you start an argument with `+` *and* prepend `no` to the option name, it is double negation, which is equivalent to enabling the option:

```sh
yash3 +o noallexport
```

### Short option names

Some shell options have short names, which are specified as a single character. For example, to start yash with the `allexport` option enabled using its short name, use an argument starting with `-`:

```sh
yash3 -a
```

To disable the `allexport` option using its short name, use an argument starting with `+`:

```sh
yash3 +a
```

Multiple short options can be specified in a single argument. For example, to enable both the `allexport`, `errexit`, and `xtrace` options at startup:

```sh
yash3 -aex
```

Some short options correspond to negation of long options. For example, the `-C` option is equivalent to `--noclobber`, which disables the `clobber` option. To enable the `clobber` option using its short name, use `+C`.

## Option list

The following is a list of all shell options available in yash-rs, along with their long and short names, and a brief description of each option. Unless otherwise noted, all options are disabled by default.

- **`allexport`** (**`-a`**): If set, all variables assigned in the shell are exported.

- **`clobber`** (**`+C`**): If set (default), the `>` [redirection](language/redirections/README.md) operator overwrites existing files. If unset, the `>` operator fails if the target file already exists. The `>|` operator always overwrites files, regardless of the `clobber` option.

- **`cmdline`** (**`-c`**): If set, the shell executes the first operand from the command line as a command. This option is mutually exclusive with the `stdin` option, and can only be set at startup. <!-- TODO: Link to startup -->

- **`errexit`** (**`-e`**): If set, the shell exits if a command fails. This is useful for scripts where you want to ensure that any command failure causes the script to stop executing. <!-- TODO: link to termination and debugging -->

- **`exec`** (**`+n`**): If set (default), the shell actually executes commands. If unset, the shell only parses commands without executing them. This is useful for syntax checking of scripts.
    - Once this option is unset, it cannot be set again in the same shell session because you cannot execute commands anymore, including the `set` built-in to set options.
    - In interactive shells, this option is ignored and commands are always executed.

- **`glob`** (**`+f`**): If set (default), the shell performs [pathname expansion](language/words/globbing.md) on words containing wildcard characters like `*`, `?`, and `[...]`. If unset, pathname expansion is skipped.

- **`hashondefinition`** (**`-h`**): This option is deprecated and has no effect. It remains for compatibility with older versions of yash and other shells.
    - Currently, the short name `-h` is a synonym for the long name `--hashondefinition`, but this may change in the future.
    - Many shells implement the `-h` option in different ways, so you cannot expect consistent behavior across different shells.

- **`ignoreeof`**: If set, the shell ignores the end-of-file (EOF) character (usually `Ctrl+D`) and does not exit. This is useful to prevent accidental exits from the shell.
    - This option takes effect only if the shell is interactive and the input is a terminal.

- **`interactive`** (**`-i`**): If set, the shell is interactive. <!-- TODO: link to interactive -->
    - This option is set automatically if....(TODO)

- **`log`**: This option is deprecated and has no effect. It remains for compatibility with older versions of yash.

## Compatibility

TODO
