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

The [exit status](commands/exit_status.md#exit-status) of a function definition is 0 if the function is defined successfully. A function definition fails with a non-zero exit status if the function name expansion fails or if there is a readonly function with the same name.

Defining functions with the `function` [reserved word](words/keywords.md) is not specified in POSIX.1-2024, and not yet implemented in yash-rs.

## Calling functions

### Function parameters

<!-- TODO: local variables -->

### Returning from functions

## Removing functions

### Readonly functions
