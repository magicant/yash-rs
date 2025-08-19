# Special parameters

**Special parameters** are predefined [parameters](index.html) that have symbolic names and provide specific information about the [shell environment]. They are not user-defined variables and cannot be assigned values with the [assignment syntax](variables.md#defining-variables).

Below are the special parameters and their meanings:

- **`@`**: All [positional parameters].
    - Expands to all positional parameters as separate fields. Useful for passing all arguments as is to a utility or [function](../functions.md).
    - When expanded outside [double quotes], the result is subject to [field splitting](../words/field_splitting.md) and [pathname expansion](../words/globbing.md). To preserve each parameter as a separate field, use `"$@"`. If there are no positional parameters, `"$@"` expands to zero fields.
    - In contexts where only one field is expected (such as in the content of a [here-document](../redirections/here_documents.md)), `@` expands to a single field with all positional parameters joined by the first character of the [`IFS` variable](variables.md#reserved-variable-names) (defaults to space if unset, or no separator if `IFS` is empty).

    ```shell
    $ set foo 'bar bar' baz # three positional parameters
    $ for value in "$@"; do echo "[$value]"; done
    [foo]
    [bar bar]
    [baz]
    $ for value in $@; do echo "[$value]"; done
    [foo]
    [bar]
    [bar]
    [baz]
    ```

- **`*`**: All [positional parameters].
    - Similar to `@`, but in [double quotes], `*` expands to a single field containing all positional parameters joined by the first character of `IFS`.

    ```shell
    $ set foo 'bar bar' baz # three positional parameters
    $ for value in "$*"; do echo "[$value]"; done
    [foo bar bar baz]
    $ for value in $*; do echo "[$value]"; done
    [foo]
    [bar]
    [bar]
    [baz]
    ```

- **`#`**: Number of [positional parameters].

    ```shell
    $ set foo 'bar bar' baz
    $ echo "$#"
    3
    ```

- **`?`**: [Exit status](../commands/exit_status.md) of the last command.

- **`-`**: Current [shell options].
    - Expands to the short names of all currently set [shell options], concatenated together. Options without a short name are omitted. For example, if `-i` and `-m` are set, the value is `im`.

- **`$`**: Process ID of the current shell.
    - Set when the shell starts and remains constant, even in [subshells](../../environment/index.html#subshells).

- **`!`**: Process ID of the last [asynchronous command](../commands/lists.md#asynchronous-commands).
    - Updated when an asynchronous command is started or [resumed in the background](../../builtins/bg.md).
    - The value is `0` until any asynchronous command is executed in the current [shell environment]. However, the behavior may be changed in the future so that it works like an unset parameter. <!-- TODO: The value is unset until any asynchronous command is executed. -->

- **`0`**: Name of the shell or script being executed.
    - Set at shell [startup](../../startup.md) and remains constant.
    - If neither the `-c` nor `-s` [shell option] is active, the value of `0` is the first operand in the [shell invocation](../../startup.md) (the script pathname).
    - If the `-c` option is used and a second operand is present, that operand is used as `0`.
    - Otherwise, `0` is set to the first argument passed to the shell, usually the shell's name.

[double quotes]: ../words/quoting.md#double-quotes
[positional parameters]: positional.md
[shell environment]: ../../environment/index.md
[shell option]: ../../environment/options.md
[shell options]: ../../environment/options.md
