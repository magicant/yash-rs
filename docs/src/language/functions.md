# Functions

A **function** is a named block of code you can call by name. Functions let you organize and reuse code in scripts and interactive sessions.

```shell
$ greet() {
>   echo "Hello, $1!"
> }
$ greet Alice
Hello, Alice!
$ greet Bob
Hello, Bob!
```

## Defining functions

To define a function, write the function name followed by parentheses `()` and a [compound command](commands/index.html#commands-1) as the body:

```shell
$ greet() {
>   echo "Hello, $1!"
> }
```

You can also write the parentheses separately, and put the body on the next line:

```shell
$ cleanup ( )
> if [ -d /tmp/myapp ]; then
>   rm -rf /tmp/myapp
> fi
```

Function names are case-sensitive and do not share a namespace with [variables](parameters/variables.md).

By POSIX.1-2024, function names must use ASCII letters, digits, and underscores, and not start with a digit. As an extension, yash-rs allows any word as a function name. The function name is [expanded](words/index.html#word-expansion) when defined:

```shell
$ "$(echo foo)"() { echo "This function is named foo."; }
$ foo
This function is named foo.
```

A function is defined when the definition command is executed, not when parsed. For example, `greet` is only defined if the current year is 2001:

```shell
$ if [ "$(date +%Y)" = 2001 ]; then
>     greet() { echo "Happy millennium!"; }
> fi
$ greet
error: cannot execute external utility "greet"
 --> <stdin>:4:1
  |
4 | greet
  | ^^^^^ utility not found
  |
```

Redirections in a function definition apply when the function is called, not when it is defined:

<!-- markdownlint-disable MD014 -->
```shell
$ dumb() { echo "Hello, $1!"; } > /dev/null
$ dumb Alice
```
<!-- markdownlint-enable MD014 -->

You can redefine a function by defining it again with the same name. The new definition replaces the old one.

The [exit status](commands/exit_status.md#exit-status) of a function definition is 0 if successful. It is non-zero if the function name expansion fails or if a [read-only function](#read-only-functions) with the same name exists.

Defining functions with the `function` [reserved word](words/keywords.md) is not POSIX and is not yet implemented in yash-rs.

### Read-only functions

Make a function read-only with the [`typeset` built-in](../builtins/typeset.md). Read-only functions cannot be redefined or removed.

```shell
$ greet() { echo "Hello, World!"; }
$ typeset -fr greet
$ greet() { echo "Hello again!"; }
error: cannot redefine read-only function `greet`
 --> <stdin>:3:1
  |
3 | greet() { echo "Hello again!"; }
  | ^^^^^ failed function redefinition
  |
 ::: <stdin>:1:1
  |
1 | greet() { echo "Hello, World!"; }
  | ----- info: existing function was defined here
  |
 ::: <stdin>:2:13
  |
2 | typeset -fr greet
  |             ----- info: existing function was made read-only here
  |
```

The [`readonly` built-in](../builtins/readonly.md) does not yet support making functions read-only in yash-rs.

### Showing function definitions

To display the definition of a function, use the [`typeset` built-in](../builtins/typeset.md) with the `-f` and `-p` options:

```shell
$ greet() {
>     echo "Hello, World!"
> }
$ typeset -fp greet
greet() { echo "Hello, World!"; }
```

## Executing functions

To run a function, specify its name as a command name in a [simple command](commands/simple.md).

```shell
$ greet() { echo "Hello, World!"; }
$ greet
Hello, World!
```

A function cannot be executed as a simple command if its name matches a [special built-in](../builtins/index.html#special-built-ins) or contains a slash. (See [command search](commands/simple.md#command-search).)
<!-- TODO: Use the command built-in to call such functions -->

### Function parameters

Fields after the function name are passed as [positional parameters](parameters/positional.md). The original positional parameters are restored when the function returns.

```shell
$ foo() {
>     echo "The function received $# arguments, which are: $*"
> }
$ set alice bob charlie
$ echo "Positional parameters before calling foo: $*"
Positional parameters before calling foo: alice bob charlie
$ foo andrea barbie cindy
The function received 3 arguments, which are: andrea barbie cindy
$ echo "Positional parameters after calling foo: $*"
Positional parameters after calling foo: alice bob charlie
```

### Returning from functions

A function runs until the end of its body or until the [`return` built-in](../builtins/return.md) is called. `return` can exit the function early and set the exit status.

```shell
$ is_positive() {
>     if [ "$1" -le 0 ]; then
>         echo "$1 is not positive."
>         return 1
>     fi
>     echo "$1 is positive."
>     return
> }
$ is_positive 5
5 is positive.
$ echo "Exit status: $?"
Exit status: 0
$ is_positive -3
-3 is not positive.
$ echo "Exit status: $?"
Exit status: 1
```

## Removing functions

Remove a function with the [`unset` built-in](../builtins/unset.md) and the `-f` option:

```shell
$ greet() { echo "Hello, World!"; }
$ unset -f greet
$ greet
error: cannot execute external utility "greet"
 --> <stdin>:3:1
  |
3 | greet
  | ^^^^^ utility not found
  |
```

## Replacing existing utilities

You can override existing utilities (except [special built-ins](../builtins/index.html#special-built-ins)) by defining a function with the same name. This is useful for customizing or extending utility behavior. To run the original utility from within your function, use the [`command` built-in](../builtins/command.md):

```shell,no_run
$ ls() {
>     command ls --color=auto "$@"
> }
$ ls
Documents  Downloads  Music  Pictures  Videos
```

## Related topics

See [Local variables](parameters/variables.md#local-variables) for temporary variables that are removed when the function returns.

See [Aliases and functions](aliases.md#aliases-and-functions) for comparison between aliases and functions.
