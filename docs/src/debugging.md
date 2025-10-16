# Script debugging

Yash-rs offers several features to help debug scripts.

## Exiting on errors

Many utilities return a non-zero exit status when they fail, but by default the shell continues executing the next command, which can lead to unexpected results. To stop the script when a command fails, enable the `errexit` option. For details, see [Exiting on errors](language/commands/exit_status.md#exiting-on-errors).

## Catching errors across pipeline components

By default, the exit status of a pipeline reflects only the last command, ignoring failures in earlier commands. To make the pipeline fail if any command fails, enable the `pipefail` [shell option]. With `pipefail`, the pipeline's exit status is that of the last command that returned a non-zero status, or zero if all returned zero. This helps catch errors in pipelines.

See the [pipeline documentation](language/commands/pipelines.md#catching-errors-across-pipeline-components) for details.

## Blocking unset parameters

Unset parameters expand to an empty string by default, which can silently hide misspelled parameter names, potentially leading to unexpected results if the intended value is unused. To catch such errors early, enable the `nounset` [shell option]. With `nounset`, the shell raises an error whenever an unset parameter is expanded. See [Unset parameters](language/words/parameters.md#unset-parameters) for more information.

This option also detects [unset variables in arithmetic expressions](arithmetic.md#variables).

## Reviewing command input

When the `verbose` [shell option] is enabled, the shell prints each command to [standard error] as it reads it, before executing. This is useful for reviewing commands being executed, especially in scripts.

```shell
$ set -o verbose
$ echo "Hello, world!"
echo "Hello, world!"
Hello, world!
```

```shell
$ set -o verbose
$ greet() {
greet() {
> echo "Hello, world!"
echo "Hello, world!"
> }
}
$ greet
greet
Hello, world!
```

## Tracing command execution

If you enable the `xtrace` [shell option], the shell prints [expanded fields](language/words/index.html#word-expansion) in each command to [standard error] before executing it. This is useful for reviewing actual commands being executed.

```shell,hidelines=#
#$ mkdir $$ && cd $$
$ set -o xtrace
$ for user in Alice Bob Charlie; do
>     echo "Hello, $user!" >> greetings.txt
> done
+ for user in Alice Bob Charlie
+ echo 'Hello, Alice!' 1>>greetings.txt
+ echo 'Hello, Bob!' 1>>greetings.txt
+ echo 'Hello, Charlie!' 1>>greetings.txt
$ cat *.txt
+ cat greetings.txt
Hello, Alice!
Hello, Bob!
Hello, Charlie!
```

<!-- markdownlint-disable-next-line MD038 -->
Each line of output is prefixed with the value of the `PS4` [variable](language/parameters/variables.md), which defaults to `+ `. [Parameter expansion](language/words/parameters.md), [command substitution](language/words/command_substitution.md), and [arithmetic expansion](language/words/arithmetic.md) are performed on the `PS4` value before printing it.

```shell
$ PS4='$((i=i+1))+ '; set -o xtrace
$ while getopts n option -n foo; do
>     case $option in
>         (n) n_option=true ;;
>         (*) echo "Unknown option: $option" ;;
>     esac
> done
1+ getopts n option -n foo
2+ case n in
3+ n_option=true
4+ getopts n option -n foo
```

Since yash 3.0.2, the `xtrace` option is ignored while expanding `PS4`. This prevents infinite recursion in case `PS4` contains a command substitution that would cause it to be expanded again.

## Checking syntax

If the `exec` [shell option] is unset, the shell only parses commands without executing them. This is useful for checking syntax errors in scripts without running them.

<!-- markdownlint-disable MD014 -->
```shell
$ set +o exec
$ echo "Hello, world!"
$ echo "Oops, a syntax error";;
error: the compound command delimiter is unmatched
 --> <stdin>:3:28
  |
3 | echo "Oops, a syntax error";;
  |                            ^^ not in a `case` command
```
<!-- markdownlint-enable MD014 -->

```sh
# Invoke the shell with the `exec` option unset to check a script file
yash3 +o exec my_script.sh
```

<!-- TODO: ## DEBUG trap: Run a command before every simple command (advanced debugging). -->

[shell option]: environment/options.md
[standard error]: language/redirections/index.html#what-are-file-descriptors
