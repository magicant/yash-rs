# Export built-in

The **`export`** built-in exports [variables](../language/parameters/variables.md) to the environment.

## Synopsis

```sh
export [-p] [name[=value]…]
```

## Description

The `export` built-in (without the `-p` option) exports variables of the specified names to the environment, with optional values. If no names are given, or if the `-p` option is given, the names and values of all exported variables are displayed. If the `-p` option is given with operands, only the specified variables are displayed.

## Options

The **`-p`** (**`--print`**) option causes the shell to display the names and
values of all exported variables in a format that can be reused as input to
restore the state of these variables. When used with operands, the option
limits the output to the specified variables.

## Operands

The operands are the names of variables to be exported or printed. When exporting, each name may optionally be followed by `=` and a *value* to assign to the variable.

## Standard output

When exporting variables, the `export` built-in does not produce any output.

When printing variables, the built-in prints [simple commands](../language/commands/simple.md) that invoke the `export` built-in to reexport the variables with the same values. Note that the commands do not include options to restore the attributes of the variables, such as the `-r` option to make variables [read-only].

For [array variables](../language/parameters/variables.md#arrays), the `export` built-in invocation is preceded by a separate assignment command since the `export` built-in does not support assigning values to array variables.

## Errors

When exporting a variable with a value, it is an error if the variable is [read-only].

When printing variables, it is an error if an operand names a non-existing variable.

## Exit status

Zero unless an error occurs.

## Examples

See [Environment variables](../language/parameters/variables.md#environment-variables).

## Compatibility

This built-in is part of the POSIX standard. Printing variables is portable
only when the `-p` option is used without operands.

Previous versions of yash supported the non-standard `-r` and `-X` options, but these are not yet supported in yash-rs.

[read-only]: ../language/parameters/variables.md#read-only-variables
