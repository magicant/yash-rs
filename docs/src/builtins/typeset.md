# Typeset built-in

The built-in behaves differently depending on the arguments.

## Defining variables

If the `-p` (`--print`) or `-f` (`--functions`) option is not specified and
there are any operands, the built-in defines shell variables named by the
operands.

Other options may be specified to set the scope and attributes of the
variables.

### Synopsis

```sh
typeset [-grx] [+rx] name[=value]...
```

### Options

By default, the built-in creates or updates variables in the current
context.  If the **`-g`** (**`--global`**) option is specified, the built-in
affects existing variables visible in the current scope (which may reside in
an outer context) or creates new variables in the base context. See the
documentation in [`yash_env::variable`] for details on the variable scope.

The following options may be specified to set the attributes of the
variables:

- **`-r`** (**`--readonly`**): Makes the variables read-only.
- **`-x`** (**`--export`**): Exports the variables to the environment.

To remove the attributes, specify the corresponding option with a plus sign
(`+`) instead of a minus sign (`-`). For example, the following commands
stop exporting the variable `foo`:

```sh
typeset +x foo
typeset ++export foo
```

Note that the read-only attribute cannot be removed, so the `+r` option is
of no use.

### Operands

Operands specify the names and values of the variables to be defined. If an
operand contains an equal sign (`=`), the operand is split into the name and
value at the first equal sign. The value is assigned to the variable named
by the name. Otherwise, the variable named by the operand is created without
a value unless it is already defined, in which case the existing value is
retained.

If no operands are given, the built-in prints variables (see below).

### Standard output

None.

# Printing variables

If the `-p` (`--print`) option is specified and the `-f` (`--functions`)
option is not specified, the built-in prints the attributes and values of
the variables named by the operands in the format that can be
[evaluated](crate::eval) as shell code to recreate the variables.
If there are no operands and the `-f` (`--functions`) option is not
specified, the built-in prints all shell variables in the same format in
alphabetical order.

## Synopsis

```sh
typeset -p [-grx] [+rx] [name...]
```

```sh
typeset [-grx] [+rx]
```

## Options

The **`-p`** (**`--print`**) option must be specified to print variables
when there are any operands. Otherwise, the built-in defines variables. The
option may be omitted if there are no operands.

By default, the built-in prints variables in the current context. If the
**`-g`** (**`--global`**) option is specified, the built-in prints variables
visible in the current scope (which may reside in an outer context).

The following options may be specified to select which variables to print.
Variables that do not match the selection criteria are ignored.

- **`-r`** (**`--readonly`**): Prints read-only variables.
- **`-x`** (**`--export`**): Prints exported variables.

If these options are negated by prefixing a plus sign (`+`) instead of a
minus sign (`-`), the built-in prints variables that do not have the
corresponding attribute.

## Operands

Operands specify the names of the variables to be printed. If no operands
are given, the built-in prints all variables that match the selection
criteria.

## Standard output

A command string that invokes the typeset built-in to recreate the variable
is printed for each variable. Exceptionally, for array variables, the
typeset command is preceded by a separate assignment command since the
typeset built-in does not support assigning values to array variables. In
this case, the typeset command is even omitted if no options are applied to
the variable.

Note that evaluating the printed commands in the current context may fail if
variables are read-only since the read-only variables cannot be assigned
values.

Below is an example of the output of the typeset built-in that displays the
variable `foo` and the read-only array variable `bar`:

```sh
typeset foo='some value that contains spaces'
bar=(this is a readonly array)
typeset -r bar
```

# Modifying functions

If the `-f` (`--functions`) option is specified, the `-p` (`--print`) option
is not specified, and there are any operands, the built-in modifies the
attributes of shell functions named by the operands.

## Synopsis

```sh
typeset -f [-r] [+r] name...
```

## Options

The **`-f`** (**`--functions`**) option is required to modify functions.
Otherwise, the built-in defines variables.

The **`-r`** (**`--readonly`**) option makes the functions read-only. If the
option is not specified, the built-in does nothing.

The built-in accepts the `+r` (`++readonly`) option, but it is of no use
since the read-only attribute cannot be removed.

## Operands

Operands specify the names of the functions to be modified. If no operands
are given, the built-in prints functions (see below).

Note that the built-in operates on existing shell functions only. It cannot
create new functions or change the body of existing functions.

## Standard output

None.

# Printing functions

If the `-f` (`--functions`) and `-p` (`--print`) options are specified, the
built-in prints the attributes and definitions of the shell functions named
by the operands in the format that can be [evaluated](crate::eval) as shell
code to recreate the functions.
If there are no operands and the `-f` (`--functions`) option is specified,
the built-in prints all shell functions in the same format in alphabetical
order.

## Synopsis

```sh
typeset -fp [-r] [+r] [name...]
```

```sh
typeset -f [-r] [+r]
```

## Options

The the **`-f`** (**`--functions`**) and **`-p`** (**`--print`**) options
must be specified to print functions when there are any operands. Otherwise,
the built-in modifies functions. The `-p` (`--print`) option may be omitted
if there are no operands.

The **`-r`** (**`--readonly`**) option can be specified to limit the output
to read-only functions. If this option is negated as `+r` (`++readonly`),
the built-in prints functions that are not read-only. If the option is not
specified, the built-in prints all functions.

## Operands

Operands specify the names of the functions to be printed. If no operands
are given, the built-in prints all functions that match the selection
criteria.

## Standard output

A command string of a function definition command is printed for each
function, which may be followed by an invocation of the typeset built-in to
set the attributes of the function.

Note that evaluating the printed commands in the current shell environment
may fail if functions are read-only since the read-only functions cannot be
redefined.

# Errors

The read-only attribute cannot be removed from a variable or function. If a
variable is already read-only, you cannot assign a value to it.

It is an error to modify a non-existing function.

When printing variables or functions, it is an error if an operand names a
non-existing variable or function.

# Exit status

Zero unless an error occurs.

# Portability

The typeset built-in is not specified by POSIX and many shells implement it
differently. This implementation is based on common characteristics seen in
other shells, but it is not fully compatible with any of them.

Some implementations allow operating on variables and functions at the same
time. This implementation does not.

This implementation requires the `-g` (`--global`) option to print variables
defined in outer contexts. Other implementations may print such variables by
default.

This implementation allows hiding a read-only variable defined in an outer
context by introducing a variable with the same name in the current context.
This may not be allowed in other implementations.

Historical versions of yash used to perform assignments when operands of the
form `name=value` are given even if the `-p` option is specified. This
implementation regards such usage as an error.

Historical versions of yash used the `-X` (`--unexport`) option to negate
the `-x` (`--export`) option. This is now deprecated because its behavior is
incompatible with other implementations. Use the `+x` (`++export`) option
instead.
