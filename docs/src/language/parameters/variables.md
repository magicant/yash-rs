# Variables

**Variables** are parameters with alphanumeric names that can be assigned values. Use variable assignment to define a variable by specifying a name and value.

## Variable names

Variable names can contain letters, digits, and underscores, but cannot start with a digit. Variable names are case-sensitive, so `VAR` and `var` are different variables.

It is common to use uppercase letters for exported (environment) variables and lowercase for local variables. This avoids accidentally overwriting environment variables.

According to POSIX.1-2024, only ASCII letters, digits, and underscores are portably accepted in variable names. Many shells allow additional characters. Yash-rs currently accepts Unicode letters and digits in variable names, but this may change in the future.

## Defining variables

To define a variable, use the assignment syntax:

```shell
$ user=Alice
```

This creates a variable named `user` with the value `Alice`. There must be no spaces around the `=` sign.

Before a value is assigned, the following expansions are performed, if any:

- Tilde expansion
- Parameter expansion
- Command substitution
- Arithmetic expansion
- Quote removal

```shell
$ topdir=~/my_project
$ subdir=$topdir/docs
$ echo $subdir
/home/alice/my_project/docs
$ rawdir='~/$user'
$ echo $rawdir
~/$user
```

Note that <!-- TODO: brace expansion, --> field splitting and pathname expansion do not happen during variable assignment.

```shell
$ star=* # assigns a literal `*` to the variable `star`
$ echo "$star" # shows the value of `star`
*
$ echo $star # unquoted, the value is subject to field splitting and pathname expansion
Documents  Downloads  Music  Pictures  Videos
```

## Environment variables

Environment variables are variables exported to child processes. To export a variable, use the `export` built-in:

```shell
$ export user=Alice
$ sh -c 'echo $user'
Alice
```

When the shell starts, it inherits environment variables from its parent. These variables are automatically exported to child processes.

## Read-only variables

The `readonly` built-in makes a variable read-only, preventing it from being modified or unset. This is useful for defining constants.

```shell
$ readonly pi=3.14
$ pi=3.14159
error: error assigning to variable
 --> <stdin>:2:1
  |
2 | pi=3.14159
  | ^^^^^^^^^^ cannot assign to read-only variable "pi"
  |
 ::: <stdin>:1:10
  |
1 | readonly pi=3.14
  |          ------- info: the variable was made read-only here
  |
```

Variables are read-only only in the current shell session. Environment variables exported to child processes are no longer read-only.

## Local variables

Variables defined by the `typeset` built-in (without the `--global` option) are local to the current shell function. Such variables are removed when the function returns.

```shell
$ i=0
$ list() {
>   typeset i
>   for i in 1 2 3; do
>     echo "Inside function: $i"
>   done
> }
$ list
Inside function: 1
Inside function: 2
Inside function: 3
$ echo "Outside function: $i"
Outside function: 0
```

As shown above, the original variable `i` is hidden by the local variable `i` inside the function and is restored when the function returns.

## Removing variables

The `unset` built-in removes a variable.

```shell
$ user=Alice
$ echo user=$user
user=Alice
$ unset user
$ echo user=$user
user=
```

Undefined variables by default expand to an empty string. Use the `-u` shell option to make the shell treat undefined variables as an error.

<!-- TODO: ## Variables used or assigned to by the shell -->
