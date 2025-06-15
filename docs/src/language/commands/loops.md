# Loops

Loops repeatedly execute a sequence of commands, either by iterating over a list or while a condition holds. They are useful for automating repetitive tasks or processing multiple items.

## For loops

A `for` loop iterates over a list of strings, executing a block of commands for each string. In each iteration, the string is assigned to a [variable] for use in the commands.

```shell
$ for user in alice bob charlie; do
>     echo "Hello, $user!"
> done
Hello, alice!
Hello, bob!
Hello, charlie!
```

The word after `for` is the loop [variable], assigned to each string in the list after `in`. The `do` [reserved word] starts the command block, and `done` ends the loop.

The semicolon after the list is optional if `do` is on a new line:

```shell
$ for user in alice bob charlie
> do
>     echo "Hello, $user!"
> done
Hello, alice!
Hello, bob!
Hello, charlie!
```

The `in` [reserved word] can also be on a separate line:

```shell
$ for user
> in alice bob charlie; do
>     echo "Hello, $user!"
> done
Hello, alice!
Hello, bob!
Hello, charlie!
```

[Word expansion](../words/index.html#word-expansion) is performed on the list:

```shell,no_run
$ for file in *.txt; do
>     echo "$file contains $(wc -l -- "$file") lines"
>     echo "First line: $(head -n 1 -- "$file")"
> done
file1.txt contains 10 lines
First line: This is the first line of file1.
file2.txt contains 5 lines
First line: This is the first line of file2.
```

If the list is empty, the loop does not run:

```shell
$ for user in; do
>     echo "Hello, $user!"
> done
```

If `in` and the list are omitted, the loop iterates over the [positional parameters](../parameters/positional.md) as if `in "$@"` were specified:

```shell
$ set alice bob charlie
$ for user do
>     echo "Hello, $user!"
> done
Hello, alice!
Hello, bob!
Hello, charlie!
```

The [exit status] of a `for` loop is the exit status of the last command run in the loop, or 0 if the loop does not run.

## While and until loops

A `while` loop executes commands as long as a condition is true. An `until` loop is similar, but continues until the condition becomes true. The `do` [reserved word] separates the condition from the loop body, and `done` ends the loop.

```shell
$ count=1
$ while [ $count -le 5 ]; do
>     echo "Count: $count"
>     count=$((count + 1))
> done
Count: 1
Count: 2
Count: 3
Count: 4
Count: 5
```

```shell
$ count=1
$ until [ $count -gt 3 ]; do
>     echo "Count: $count"
>     count=$((count + 1))
> done
Count: 1
Count: 2
Count: 3
```

See [Exit status and conditionals](exit_status.md) for details on how exit status affects loop conditions.

The [exit status] of a `while` or `until` loop is that of the last command run in the loop body, or 0 if the loop body does not run. Note that the exit status of the condition does not affect the exit status of the loop.

## Break and continue

The `break` utility exits the current loop. The `continue` utility skips to the next iteration.

```shell
$ for i in 1 2 3 4 5; do
>     if [ $i -eq 3 ]; then
>         echo "Breaking at $i"
>         break
>     fi
>     echo "Iteration $i"
> done
Iteration 1
Iteration 2
Breaking at 3
```

```shell
$ for i in 1 2 3 4 5; do
>     if [ $i -eq 3 ]; then
>         echo "Skipping $i"
>         continue
>     fi
>     echo "Iteration $i"
> done
Iteration 1
Iteration 2
Skipping 3
Iteration 4
Iteration 5
```

By default, `break` and `continue` affect the innermost loop. You can specify a numeric operand to affect the *n*'th outer loop:

```shell
$ for i in 1 2; do
>     for j in a b c; do
>         if [ "$j" = "b" ]; then
>             echo "Breaking outer loop at $i, $j"
>             break 2
>         fi
>         echo "Inner loop: $i, $j"
>     done
> done
Inner loop: 1, a
Breaking outer loop at 1, b
```

[exit status]: exit_status.md
[reserved word]: ../words/keywords.md
[variable]: ../parameters/variables.md
