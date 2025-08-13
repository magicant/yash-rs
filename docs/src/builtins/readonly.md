# Readonly built-in

The **`readonly`** built-in provides several operations related to [read-only variables](../language/parameters/variables.md#read-only-variables):

- [Making variables read-only](#making-variables-read-only)
- [Printing read-only variables](#printing-read-only-variables)
<!--
- [Making functions read-only](#making-functions-read-only)
- [Printing read-only functions](#printing-read-only-functions)
-->

## Making variables read-only

If the `-p` (`--print`)<!-- TODO: or `-f` (`--functions`) --> option is not specified and there are any operands, the built-in makes the specified variables read-only.

### Synopsis

```sh
readonly name[=value]…
```

### Options

None.

### Operands

Operands specify the names and values of the [variables] to be made read-only. If an operand contains an equal sign (`=`), the operand is split into the name and value at the first equal sign. The value is assigned to the variable named by the name. Otherwise, the variable named by the operand is created without a value unless it is already defined, in which case the existing value is retained.

If no operands are given, the built-in prints variables (see below).

### Standard output

None.

## Printing read-only variables

If the `-p` (`--print`) option is specified<!-- TODO: and the `-f` (`--functions`) option is not specified -->, the built-in prints the names and values of the variables named by the operands in the format that can be [evaluated](eval.md) as shell code to recreate the variables. If there are no operands<!-- TODO: and the `-f` (`--functions`) option is not specified-->, the built-in prints all read-only variables in the same format.

### Synopsis

```sh
readonly -p [name…]
```

```sh
readonly
```

### Options

The **`-p`** (**`--print`**) option must be specified to print variables unless there are no operands.

### Operands

Operands specify the names of the variables to be printed. If no operands are given, all read-only variables are printed.

### Standard output

A command string that invokes the `readonly` built-in to recreate the variable is printed for each read-only variable. Note that the command does not include options to restore the attributes of the variable, such as the `-x` option to make variables exported.

Also note that evaluating the printed commands in the current shell session will fail (unless the variable is declared without a value) because the variable is already defined and read-only.

For array variables, the built-in invocation is preceded by a separate assignment command since the built-in does not support assigning values to array variables.

<!-- TODO
## Making functions read-only

If the `-f` (`--functions`) option is specified, the built-in makes the specified functions read-only.

### Synopsis

```sh
readonly -f name…
```

### Options

The **`-f`** (**`--functions`**) option must be specified to make functions
read-only.

### Operands

Operands specify the names of the functions to be made read-only.

### Standard output

None.

## Printing read-only functions

If the `-f` (`--functions`) and `-p` (`--print`) options are specified, the built-in prints the attributes and definitions of the shell functions named by the operands in the format that can be [evaluated](crate::eval) as shell code to recreate the functions. If there are no operands and the `-f` (`--functions`) option is specified, the built-in prints all read-only functions in the same format.

### Synopsis

```sh
readonly -fp [name…]
```

```sh
readonly -f
```

### Options

The **`-f`** (**`--functions`**) and **`-p`** (**`--print`**) options must be specified to print functions. The `-p` option may be omitted if there are no operands.

### Operands

Operands specify the names of the functions to be printed. If no operands are given, all read-only functions are printed.

### Standard output

A command string of a function definition command is printed for each function, followed by a simple command invoking the `readonly` built-in to make the function read-only.

Note that executing the printed commands in the current context will fail because the function is already defined and read-only.
-->

## Errors

When making a variable read-only with a value, it is an error if the variable is already read-only.

<!-- TODO: It is an error to specify a non-existing function for making it read-only. -->

When printing variables<!-- TODO: or functions -->, it is an error if an operand names a non-existing variable<!-- TODO: or function -->.

## Exit status

Zero if successful, non-zero if an error occurred.

## Examples

```shell
$ readonly foo='Hello, world!'
$ echo "$foo"
Hello, world!
$ readonly
readonly foo='Hello, world!'
$ foo='Goodbye, world!'
error: error assigning to variable
 --> <stdin>:4:1
  |
4 | foo='Goodbye, world!'
  | ^^^^^^^^^^^^^^^^^^^^^ cannot assign to read-only variable "foo"
  |
 ::: <stdin>:1:10
  |
1 | readonly foo='Hello, world!'
  |          ------------------- info: the variable was made read-only here
  |
```

## Compatibility

This built-in is part of the POSIX standard. Printing variables is portable only when the `-p` option is used without operands. <!-- TODO: Operations on functions with the `-f` option are non-portable extensions. -->

[variables]: ../language/parameters/variables.md
