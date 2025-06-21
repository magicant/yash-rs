# Functions

A **function** is a named block of code that can be executed by calling its name. Functions allow you to encapsulate code for reuse, making scripts and interactive sessions more organized and efficient.

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

To define a function, the function name is followed by parentheses `()` and a [compound command](commands/index.html#commands-1). The compound command is the function body, which is the code that runs when the function is called.

```shell
$ greet() {
>   echo "Hello, $1!"
> }
```

Parentheses may appear separately and the function body may be on the next line:

```shell
$ cleanup ( )
> if [ -d /tmp/myapp ]; then
>   rm -rf /tmp/myapp
> fi
```

Functions do not share namespaces with [variables](parameters/variables.md). Function names are case-sensitive.

In POSIX.1-2024, function names must consist of ASCII letters, digits, and underscores, and must not start with a digit. As an extension, yash-rs currently allows any word as a function name. The function name word is [expanded](words/index.html#word-expansion) when the function is defined.

```shell
$ "$(echo foo)"() { echo "This function is named foo."; }
$ foo
This function is named foo.
```

The function is defined when the function definition command is executed, not when parsed. In the example below, the function `greet` is defined only if the current year is 2001.

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

Redirections in function definitions applies to the function body when the function is called, not when it is defined:

```shell
$ dumb() { echo "Hello, $1!"; } > /dev/null
$ dumb Alice
```

Functions can be redefined by defining a function with the same name again. The new definition replaces the old one.

The [exit status](commands/exit_status.md#exit-status) of a function definition is 0 if the function is defined successfully. A function definition fails with a non-zero exit status if the function name expansion fails or if there is a readonly function with the same name.

Defining functions with the `function` [reserved word](words/keywords.md) is not specified in POSIX.1-2024, and not yet implemented in yash-rs.

### Readonly functions

Functions can be made readonly with the `typeset` built-in. Readonly functions cannot be redefined or removed.

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

The `readonly` built-in does not yet support making functions readonly in yash-rs.

## Executing functions

To execute a function, specify the function name as a command name in a [simple command](commands/simple.md).

```shell
$ greet() { echo "Hello, World!"; }
$ greet
Hello, World!
```

Because of the [command search](commands/simple.md#command-search) algorithm, a function cannot be executed with a simple command if the function name matches a special built-in name or contains a slash.
<!-- TODO: Use the command built-in to call such functions -->

### Function parameters

The fields other than the first in the simple command are passed as parameters to the function. The parameters are accessible within the function body as [positional parameters](parameters/positional.md). The original positional parameters are restored when the function returns.

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

Execution of a function continues until it reaches the end of the function body or an invocation of the `return` built-in. The `return` built-in can be used to exit the function early and optionally specify an exit status.

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

To remove a function, use the `unset` built-in with the `-f` option.

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

## Related topics

See [Local variables](parameters/variables.md#local-variables) to define temporary variables that are removed when the function returns.

<!-- TODO Aliases -->
