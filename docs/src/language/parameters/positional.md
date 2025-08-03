# Positional parameters

**Positional parameters** are parameters identified by their position in the command line. They are commonly used to pass arguments to scripts or functions.

## Initializing positional parameters

Positional parameters are set when the shell starts:

- If neither the `-c` nor `-s` shell option is active, positional parameters are set to the operands after the first operand in the shell invocation. For example:

  ```sh
  yash3 script.sh arg1 arg2 arg3
  ```

  Here, the positional parameters are `arg1`, `arg2`, and `arg3`.

- If the `-c` option is used, positional parameters are set to operands after the second operand, if any:

  ```sh
  yash3 -c 'echo "$1" "$2"' arg0 arg1 arg2
  ```

  The positional parameters are `arg1` and `arg2`. The second operand (`arg0`) is used as [special parameter] `0`, not as a positional parameter.

- If the `-s` option is active, positional parameters are set to all operands in the shell invocation:

  ```sh
  yash3 -s arg1 arg2 arg3
  ```

  The positional parameters are `arg1`, `arg2`, and `arg3`.

## Modifying positional parameters

To set positional parameters, use the `set` built-in:

```shell
$ set foo bar baz
$ echo "$1" "$2" "$3"
foo bar baz
```

To append new parameters without removing existing ones, use `set -- "$@"` followed by the new parameters:

```shell
$ set old_param1 old_param2
$ set -- "$@" new_param1 new_param2
$ echo "$1" "$2" "$3" "$4"
old_param1 old_param2 new_param1 new_param2
```

The `--` marks the end of options, so parameters starting with `-` are not treated as options.

To remove the first N positional parameters, use `shift`:

```shell
$ set foo bar baz qux
$ echo "$1" "$2" "$3" "$4"
foo bar baz qux
$ shift 2
$ echo "$1" "$2"
baz qux
```

If `set` is called with no operands, positional parameters are unchanged. To clear them, use `set --` or `shift "$#"`.

When a function is called, positional parameters are set to the function's arguments. You can modify them within the function using `set` or `shift`. After the function returns, the original positional parameters are restored.
<!-- TODO: positional parameters in dot scripts -->

## Expanding positional parameters

In [parameter expansion](../words/parameters.md), positional parameters are referenced by their position, starting from `1`:

```shell
$ set foo bar baz
$ echo "$3" "$2" "$1"
baz bar foo
```

For positions above `9`, use braces:

```shell
$ set a b c d e f g h i j k l m n o p q r s t u v w x y z
$ echo "${1}" "${10}" "${26}"
a j z
$ echo "$10" # expands as ${1}0
a0
```

To expand all positional parameters at once, you can use the [special parameter] `@` or `*`. Specifically, to pass all positional parameters intact to a utility or function, expand `@` in double quotes:

```shell
$ set foo 'bar bar' baz
$ printf '[%s]\n' "$@"
[foo]
[bar bar]
[baz]
```

To get the number of positional parameters, use the [special parameter] `#`:

```shell
$ set foo bar baz
$ echo "$#"
3
```

## Parsing positional parameters

To parse positional parameters as options and arguments, use the [`getopts` built-in](../../builtins/getopts.md). This is useful for scripts that handle command-line options:

```shell
$ set -- -a arg1 -b arg2 operand1 operand2
$ while getopts a:b: opt; do
>   case "$opt" in
>     (a)
>       echo "Option -a with argument: $OPTARG"
>       ;;
>     (b)
>       echo "Option -b with argument: $OPTARG"
>       ;;
>     (*)
>       echo "Unknown option: $opt"
>       ;;
>   esac
> done
Option -a with argument: arg1
Option -b with argument: arg2
$ shift $((OPTIND - 1)) # remove parsed options
$ echo "Remaining operands:" "$@"
Remaining operands: operand1 operand2
```

[special parameter]: special.md
