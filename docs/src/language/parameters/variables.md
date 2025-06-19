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

- [Tilde expansion](../words/tilde.md)
- [Parameter expansion](../words/parameters.md)
- [Command substitution](../words/command_substitution.md)
- [Arithmetic expansion](../words/arithmetic.md)
- [Quote removal](../words/quoting.md#quote-removal)

```shell,hidelines=#
#$ HOME=/home/alice
$ topdir=~/my_project
$ subdir=$topdir/docs
$ echo $subdir
/home/alice/my_project/docs
$ rawdir='~/$user'
$ echo $rawdir
~/$user
```

Note that <!-- TODO: brace expansion, --> [field splitting](../words/field_splitting.md) and [pathname expansion](../words/globbing.md) do not happen during variable assignment.

```shell,no_run
$ star=* # assigns a literal `*` to the variable `star`
$ echo "$star" # shows the value of `star`
*
$ echo $star # unquoted, the value is subject to field splitting and pathname expansion
Documents  Downloads  Music  Pictures  Videos
```

See the [Simple commands](../commands/simple.md) section for more information on assignment behavior.

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

Variables defined by the `typeset` built-in (without the `--global` option) are local to the current shell [function](../functions.md). Such variables are removed when the function returns.

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

## Reserved variable names

Some variable names are reserved for special purposes. These variables may affect or be affected by the shell's behavior.

- **`CDPATH`**: A colon-separated list of directories to search in the `cd` built-in

- **`ENV`**: The name of a file to be sourced when starting an interactive shell

- **`HOME`**: The user's home directory, used in [tilde expansion](../words/tilde.md)

- **`IFS`**: A list of delimiters used in [field splitting](../words/field_splitting.md)
    - The default value is a space, tab, and newline.

- **`LINENO`**: The current line number in the shell script
    - This variable is automatically updated as the shell executes commands.
    - Currently, yash-rs does not support exporting this variable.

- **`OLDPWD`**: The previous working directory, updated by the `cd` built-in

- **`OPTARG`**: The value of the last option argument processed by the `getopts` built-in

- **`OPTIND`**: The index of the next option to be processed by the `getopts` built-in

- **`PATH`**: A colon-separated list of directories to search for executable files when running external utilities

- **`PPID`**: The process ID of the parent process of the shell
    - This variable is initialized when the shell starts.

- **`PS1`**: The primary prompt string, displayed before each command in interactive mode
    - The default value is `$ ` (a dollar sign followed by a space). <!-- TODO: The default value should be `# ` for the root user. -->

- **`PS2`**: The secondary prompt string, displayed when a command is continued on the next line
    - The default value is `> ` (a greater-than sign followed by a space).

- **`PS4`**: The pseudo-prompt string, used for debugging output
    - The default value is `+ ` (a plus sign followed by a space).

- **`PWD`**: The current working directory
    - This variable is initialized to the working directory when the shell starts and updated by the `cd` built-in when changing directories.

## Arrays

Arrays are variables that can hold multiple values.

### Defining arrays

To define an array, wrap the values in parentheses in the assignment syntax:

```shell
$ fruits=(apple banana cherry)
```

### Accessing array elements

Unfortunately, accessing individual elements of an array is not yet implemented in yash-rs.

To access all elements of an array, just use the array name in [parameter expansion](../words/parameters.md):

```shell,hidelines=#
#$ fruits=(apple banana cherry)
$ for fruit in "$fruits"; do echo "$fruit"; done
apple
banana
cherry
```

<!-- TODO
### Array length

To get the length of an array, use the `${#array[@]}` syntax:

```shell,ignore
$ echo "${#fruits[@]}"
3
```

### Array slicing

You can slice arrays using the `${array[@]:start:length}` syntax:

```shell,ignore
$ echo "${fruits[@]:1:2}"
banana cherry
```

### Array operations

Yash-rs provides several built-in operations for working with arrays, such as adding or removing elements, concatenating arrays, and more.

```shell,ignore
$ fruits+=(date)
$ echo "${fruits[@]}"
apple banana cherry date
-->
