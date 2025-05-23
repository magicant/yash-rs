# Comments

Use comments to add notes or explanations to shell scripts and commands. The shell ignores comments during execution.

A comment starts with the `#` character and continues to the end of the line.

```shell
$ # This is a comment
$ echo "Hello, world!"  # This prints a message
Hello, world!
```

Always separate the start of a comment from the preceding word with whitespace. If there is no whitespace, the shell treats the `#` as part of the word, not as a comment.

```shell
$ echo "Hello, world!"# This is not a comment
Hello, world!# This is not a comment
```

Everything after `#` on the same line is ignored by the shell. You cannot use [line continuation](quoting.md#line-continuation) inside comments.

```shell
$ echo one # This backslash is not a line continuation 👉 \
one
$ echo two # So this line is a separate command
two
```
