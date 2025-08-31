# Dynamic command evaluation

Two built-in utilities support dynamic command evaluation.

## Evaluating command strings

The [`eval` built-in](builtins/eval.md) evaluates a command string. This is useful for constructing and executing commands dynamically.

For example, you can use `eval` to assign a value to a variable whose name is chosen at runtime:

```sh
echo "Type a variable name:"
read -r varname
eval "$varname='Hello, world!'"
eval "echo 'The value of $varname is:' \$$varname"
```

## Reading and executing files

The [`.` (dot) built-in](builtins/source.md) reads and executes commands from a file. This is useful for organizing scripts and reusing code.

For example, you can use `.` to source a file containing variable definitions:

```sh
# contents of vars.sh
greeting="Hello, world!"
farewell="Goodbye, world!"

# main script
. ./vars.sh
echo "$greeting"
echo "$farewell"
```

`source` is a non-POSIX synonym for the `.` built-in.
