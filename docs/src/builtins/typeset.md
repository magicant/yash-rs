# Typeset built-in

The **`typeset`** built-in provides several operations related to [variables](../language/parameters/variables.md) and [functions](../language/functions.md):

- [Defining variables](#defining-variables)
- [Printing variables](#printing-variables)
- [Modifying function attributes](#modifying-function-attributes)
- [Printing functions](#printing-functions)

## Defining variables

If neither the `-p` (`--print`) nor the `-f` (`--functions`) option is specified and there are operands, the built-in defines variables named by the operands.

You can specify additional options to set the scope and attributes of the variables.

### Synopsis

```sh
typeset [-grx] [+rx] name[=value]…
```

### Options

By default, the built-in creates or updates variables [locally within the current function](../language/parameters/variables.md#local-variables). If the **`-g`** (**`--global`**) option is specified, the built-in affects existing variables visible in the current scope (which may be outside the current function) or creates new variables globally.

The following options set variable attributes:

- **`-r`** (**`--readonly`**): Makes the variables [read-only](../language/parameters/variables.md#read-only-variables).
- **`-x`** (**`--export`**): [Exports](../language/parameters/variables.md#environment-variables) the variables to the environment.

To remove these attributes, use the corresponding option with a plus sign (`+`) instead of a minus sign (`-`). For example, the following commands stop exporting the variable `foo`:

```sh
typeset +x foo
typeset ++export foo
```

Note: The read-only attribute cannot be removed. Using the `+r` option to read-only variables causes an error.

### Operands

Operands specify the names and values of the variables to define. If an operand contains an equal sign (`=`), it is split at the first equal sign into a name and a value. The value is assigned to the variable with that name. If an operand does not contain an equal sign, the variable is created without a value unless it already exists, in which case its value is retained.

If no operands are given, the built-in prints variables ([see below](#printing-variables)).

### Standard output

None.

### Examples

See [Local variables](../language/parameters/variables.md#local-variables) for examples of defining local variables.

The following example demonstrates defining a local read-only variable:

```shell
$ greet() {
>     typeset -r user=Alice
>     echo "Hello, $user!"
> }
$ greet
Hello, Alice!
$ user=Bob
$ echo "Now the user is $user."
Now the user is Bob.
```

## Printing variables

If the `-p` (`--print`) option is specified and the `-f` (`--functions`) option is not, the built-in prints the attributes and values of the variables named by the operands, using a format that can be [evaluated](../dynamic_evaluation.md#evaluating-command-strings) as shell code to recreate the variables. If there are no operands and the `-f` (`--functions`) option is not specified, the built-in prints all variables in the same format, in alphabetical order.

### Synopsis

```sh
typeset -p [-grx] [+rx] [name…]
```

```sh
typeset [-grx] [+rx]
```

### Options

The **`-p`** (**`--print`**) option must be specified to print variables when operands are given. Otherwise, the built-in defines variables. If there are no operands, the option may be omitted.

By default, the built-in prints variables in the current context. If the **`-g`** (**`--global`**) option is specified, it prints variables visible in the current scope (which may be outside the current function).

The following options filter which variables are printed. Variables that do not match the criteria are ignored.

- **`-r`** (**`--readonly`**): Prints only [read-only variables](../language/parameters/variables.md#read-only-variables).
- **`-x`** (**`--export`**): Prints only [exported variables](../language/parameters/variables.md#environment-variables).

If these options are negated by using a plus sign (`+`) instead of a minus sign (`-`), the built-in prints variables that do not have the corresponding attribute. For example, the following command shows all non-exported variables:

```sh
typeset -p +x
```

### Operands

Operands specify the names of the variables to print. If no operands are given, the built-in prints all variables that match the selection criteria.

### Standard output

For each variable, a command string that invokes the `typeset` built-in to recreate the variable is printed. For array variables, a separate assignment command precedes the `typeset` command, since the built-in does not support assigning values to arrays. In this case, the `typeset` command is omitted if no options are applied to the variable.

Note: Evaluating the printed commands in the current context may fail if variables are read-only, since read-only variables cannot be assigned values.

### Examples

```shell
$ foo='some value that contains spaces'
$ bar=(this is a readonly array)
$ typeset -r bar
$ typeset -p foo bar
typeset foo='some value that contains spaces'
bar=(this is a readonly array)
typeset -r bar
```

## Modifying function attributes

If the `-f` (`--functions`) option is specified, the `-p` (`--print`) option is not specified, and there are operands, the built-in modifies the attributes of [functions](../language/functions.md) named by the operands.

### Synopsis

```sh
typeset -f [-r] [+r] name…
```

### Options

The **`-f`** (**`--functions`**) option is required to modify functions. Otherwise, the built-in defines variables.

The **`-r`** (**`--readonly`**) option makes the functions [read-only](../language/functions.md#read-only-functions). If this option is not specified, the built-in does nothing.

The built-in accepts the `+r` (`++readonly`) option, but it has no effect since the read-only attribute cannot be removed.

### Operands

Operands specify the names of the functions to modify. If no operands are given, the built-in prints functions ([see below](#printing-functions)).

Note: The built-in operates only on existing functions. It cannot create new functions or change the body of existing functions.

### Standard output

None.

### Examples

See [Read-only functions](../language/functions.md#read-only-functions).

## Printing functions

If both the `-f` (`--functions`) and `-p` (`--print`) options are specified, the built-in prints the attributes and definitions of the functions named by the operands, using a format that can be [evaluated](../dynamic_evaluation.md#evaluating-command-strings) as shell code to recreate the functions. If there are no operands and the `-f` (`--functions`) option is specified, the built-in prints all functions in the same format, in alphabetical order.

### Synopsis

```sh
typeset -fp [-r] [+r] [name…]
```

```sh
typeset -f [-r] [+r]
```

### Options

The **`-f`** (**`--functions`**) and **`-p`** (**`--print`**) options must both be specified to print functions when operands are given. Otherwise, the built-in modifies functions. The `-p` (`--print`) option may be omitted if there are no operands.

The **`-r`** (**`--readonly`**) option can be specified to limit the output to [read-only functions](../language/functions.md#read-only-functions). If this option is negated as `+r` (`++readonly`), the built-in prints functions that are not read-only. If the option is not specified, all functions are printed.

### Operands

Operands specify the names of the functions to print. If no operands are given, the built-in prints all functions that match the selection criteria.

### Standard output

For each function, a [function definition command](../language/functions.md#defining-functions) is printed, which may be followed by a `typeset` command to set the function's attributes. The output is formatted by the shell and may differ from the original function definition.

Note: Evaluating the printed commands in the current shell environment may fail if functions are read-only, since read-only functions cannot be redefined.

<!-- markdownlint-disable MD033 -->
<p class="warning">
Currently, yash-rs does not print the contents of <a href="../language/redirections/here_documents.md">here-documents</a>. Functions containing here-documents are not correctly recreated when the output is evaluated.
</p>
<!-- markdownlint-enable MD033 -->

### Examples

See [Showing function definitions](../language/functions.md#showing-function-definitions).

## Errors

The read-only attribute cannot be removed from a variable or function. If a variable is already read-only, you cannot assign a value to it.

It is an error to modify a non-existent function.

When printing variables or functions, it is an error if an operand names a non-existent variable or function.

## Exit status

Zero if successful; non-zero if an error occurs.

## Additional notes

The `-g` (`--global`) option has no effect if the built-in is used outside a function.

<!-- TODO Mention the local built-in -->

## Compatibility

The `typeset` built-in is not specified by POSIX, and many shells implement it differently. This implementation is based on common characteristics found in other shells, but it is not fully compatible with any of them.

Some implementations allow operating on variables and functions at the same time. This implementation does not.

This implementation requires the `-g` (`--global`) option to print variables defined outside the current function. Other implementations may print such variables by default.

This implementation allows hiding a read-only variable defined outside the current function by introducing a variable with the same name in the current function. Other implementations may not allow this.

Historical versions of yash performed assignments when operands of the form `name=value` were given, even if the `-p` option was specified. This implementation treats such usage as an error.

Historical versions of yash used the `-X` (`--unexport`) option to negate the `-x` (`--export`) option. This is now deprecated because its behavior was incompatible with other implementations. Use the `+x` (`++export`) option instead.
