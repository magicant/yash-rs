# Redirections

**Redirections** control where command input and output go. They let you save output to files, read input from files, or otherwise manipulate how commands perform I/O operations.

## What are file descriptors?

A **file descriptor** is a non-negative integer that identifies an open file or I/O channel in a process. When a process opens a file, the operating system assigns it a file descriptor, which the process uses to read from or write to that file.

The first three file descriptors have standard meanings:

- **0**: Standard input – the source of input data
- **1**: Standard output – the destination for command results
- **2**: Standard error – the destination for error messages and diagnostics

By default, these are connected to the terminal, but they can be redirected to files or other destinations.

## Redirection syntax

A redirection consists of a special operator followed by a target (such as a file or file descriptor). Redirections can appear anywhere in a [simple command](../commands/simple.md), or after the body of a [compound command](../commands/index.html#commands-1).

For example, the `>` operator redirects standard output to a file:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ echo "Hello, World!" > output.txt
$ cat output.txt
Hello, World!
```

The `<` operator redirects standard input from a file:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ printf 'One\nTwo\nThree\n' > input.txt
$ while read -r line; do
>     echo "Read: $line"
> done < input.txt
Read: One
Read: Two
Read: Three
```

## Redirection operators

Yash-rs supports these redirection operators:

- **`<`** (file): Redirects standard input from a file.

- **`>`** (file): Redirects standard output to a file.
    - If the `clobber` shell option is set (default), `>` behaves like `>|`.
    - If `clobber` is not set, `>` fails if the file exists and is a regular file or a symlink to a non-existent file. Otherwise, it creates a new file or opens the existing one. This is useful for preventing accidental overwriting of files.

    ```shell,hidelines=#
    #$ mkdir $$ && cd $$ || exit
    $ set -o noclobber
    $ echo "Hello, World!" > file.txt
    $ cat file.txt
    Hello, World!
    $ echo "Another redirection" > file.txt
    error: cannot open the file
     --> <stdin>:5:30
      |
    5 | echo "Another redirection" > file.txt
      |                              ^^^^^^^^ file.txt: File exists (os error 17)
      |
    ```

- **`>|`** (file): Redirects standard output to a file, overwriting it if it exists.
    - Always overwrites existing files, regardless of the `clobber` option.
    - Truncates the file if it exists, or creates it if not.

    ```shell,hidelines=#
    #$ mkdir $$ && cd $$ || exit
    $ echo "Hello, World!" >| file.txt
    $ cat file.txt
    Hello, World!
    $ echo "Another redirection" >| file.txt
    $ cat file.txt
    Another redirection
    ```

- **`>>`** (file): Redirects standard output to a file, appending to it if it exists.
    - Appends to the file if it exists, or creates it if not.

    ```shell,hidelines=#
    #$ mkdir $$ && cd $$ || exit
    $ echo "Hello, World!" >> file.txt
    $ cat file.txt
    Hello, World!
    $ echo "Another line" >> file.txt
    $ cat file.txt
    Hello, World!
    Another line
    ```

- **`<>`** (file): Opens a file for both reading and writing.
    - Opens the file if it exists, or creates it if not.

- **`<&`**: Duplicates or closes standard input, depending on the target word:
    - If the word is a file descriptor number, standard input becomes a copy of that descriptor (which must be open for reading).
    - If the word is `-`, standard input is closed. No error is reported if it is already closed.
        - If standard input is closed, commands that read from it will fail. To provide empty input, use `< /dev/null` instead.

- **`>&`**: Duplicates or closes standard output, depending on the target word:
    - If the word is a file descriptor number, standard output becomes a copy of that descriptor (which must be open for writing).
    - If the word is `-`, standard output is closed. No error is reported if it is already closed.
        - If standard output is closed, commands that write to it will fail. To discard output, use `> /dev/null` instead.
    - For example, `>&2` redirects standard output to standard error:

      ```shell
      $ echo "error: please specify a user" >&2
      error: please specify a user
      ```

<!-- TODO: pipe redirection -->
<!-- TODO: here-string -->

- **`<<`** (delimiter): Opens a [here-document](here_documents.md).

- **`<<-`** (delimiter): Opens a [here-document](here_documents.md) with automatic removal of leading tabs.

## Specifying file descriptors

Redirection operators starting with `<` default to standard input; those starting with `>` default to standard output. To redirect a different descriptor, prefix the operator with its number (no space):

For example, to redirect standard error to a file:

```shell,no_run
$ grep "pattern" input.txt 2> error.log
```

If you insert a space, the number is treated as a command argument, not a file descriptor:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ echo 2 > output.txt
$ cat output.txt
2
```

<!-- TODO: IO_NUMBER recognition in <1<2 -->

Some shells allow using a [variable](../parameters/variables.md) name in braces `{}` instead of a file descriptor. For file-opening redirections, the shell allocates a new descriptor and assigns it to the variable. For descriptor-copying redirections, the shell uses the descriptor stored in the variable.

This is not yet implemented in yash-rs, but would look like:

```shell,ignore
$ exec {fd}> output.txt
$ echo "Hello, World!" >&$fd
$ cat output.txt
Hello, World!
```

## Target word expansion

Except for here-documents, the word after a redirection operator is expanded before use. The following expansions are performed:

- [Tilde expansion](../words/tilde.md)
- [Parameter expansion](../words/parameters.md)
- [Command substitution](../words/command_substitution.md)
- [Arithmetic expansion](../words/arithmetic.md)
- [Quote removal](../words/quoting.md#quote-removal)

[Pathname expansion](../words/globbing.md) may be supported in the future.

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ file="output.txt"
$ echo "Hello, World!" > "$file"
$ cat "$file"
Hello, World!
```

## Persistent redirections

By default, redirections only apply to the command they are attached to. To make a redirection persist across multiple commands, use the `exec` built-in without arguments:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ exec > output.txt
$ echo "Hello, World!"
$ echo "More greetings!"
$ cat output.txt >&2
Hello, World!
More greetings!
```

You can use the `>&` operator to save a file descriptor before redirecting it, and restore it later:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ exec 3>&1 # Save current standard output to file descriptor 3
$ exec > output.txt # Redirect standard output to a file
$ echo "Hello, World!"
$ exec >&3 # Restore standard output from file descriptor 3
$ exec 3>&- # Close file descriptor 3
$ echo "More greetings!"
More greetings!
$ cat output.txt
Hello, World!
```

## Semantic details

Applying a redirection to a compound command is different from applying it to a simple command inside the compound command. Each use of `<` opens a new file descriptor at the start of the file. If the redirection is inside a loop, the file descriptor is reset to the beginning on each iteration:

```shell,no_run
$ printf 'One\nTwo\nThree\n' > input.txt
$ while read -r line < input.txt; do
>     echo "Read: $line"
> done
Read: One
Read: One
Read: One
Read: One
Read: One
(The loop never ends…)
```

If a command has multiple redirections, they are applied in order. If several affect the same file descriptor, the last one takes effect:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ echo "Hello, World!" > dummy.txt > output.txt 2> error.txt
$ cat dummy.txt
$ cat output.txt
Hello, World!
$ cat error.txt
```

Note the difference between `> /dev/null 2>&1` and `2>&1 > /dev/null`:

- `> /dev/null 2>&1` redirects both standard output and standard error to `/dev/null`, discarding both.
- `2>&1 > /dev/null` redirects standard error to standard output, and then redirects standard output to `/dev/null`. This means standard error is still printed to the terminal (or wherever standard output was originally directed).

```shell,no_run
$ cat /nonexistent/file > /dev/null 2>&1
$ cat /nonexistent/file 2>&1 > /dev/null
cat: /nonexistent/file: No such file or directory
```
