# Reserved words

Some words have special meaning in shell syntax. These **reserved words** must be [quoted](quoting.md) to use them literally. The reserved words are:

- `!` – [Negation](../commands/pipelines.md#negation)
- `{` – Start of a [grouping](../commands/grouping.md#braces)
- `}` – End of a [grouping](../commands/grouping.md#braces)
- `[[` – Start of a double bracket command
- `]]` – End of a double bracket command (since 3.3.0)
- `case` – [Case command](../commands/case.md)
- `do` – Start of a loop or conditional block
- `done` – End of a loop or conditional block
- `elif` – Else if clause
- `else` – Else clause
- `esac` – End of a [case command](../commands/case.md)
- `fi` – End of an [if command](../commands/exit_status.md#if-commands)
- `for` – [For loop](../commands/loops.md#for-loops)
- `function` – [Function](../functions.md) definition
- `if` – [If command](../commands/exit_status.md#if-commands)
- `in` – Delimiter for a [for loop](../commands/loops.md#for-loops) and [case command](../commands/case.md)
- `namespace` – Namespace declaration (since 3.3.0)
- `select` – Select command (since 3.3.0)
- `then` – Then clause
- `until` – [Until loop](../commands/loops.md#while-and-until-loops)
- `while` – [While loop](../commands/loops.md#while-and-until-loops)

The reserved words `[[`, `]]`, `function`, `namespace`, and `select` are recognized but not yet implemented. Using these reserved words will result in a syntax error.

Additionally, the POSIX standard allows for the following optional reserved words:

- `time` – Time command
- Any words that end with a colon (`:`)

These words are not reserved in yash-rs now, but may be in the future. (Since 3.3.0) The [`portable` option](../../environment/options.md#portable) rejects a command name ending with a `:` (the lone `:` [colon built-in](../../builtins/colon.md) is exempt).

## Where are reserved words recognized?

Reserved words are recognized in these contexts:

- As the first word of a [command](../commands/index.html#commands-1)
- As a word following any reserved word other than `case`, `for`, or `in`
- `in` as the third word in a [for loop](../commands/loops.md#for-loops) or [case command](../commands/case.md)
- `do` as the third word in a [for loop](../commands/loops.md#for-loops)

As an extension, yash-rs additionally recognizes a reserved word immediately after a subshell or a redirection, which POSIX does not, so such scripts are not portable. (Since 3.3.0) The [`portable` option](../../environment/options.md#portable) rejects a reserved word in this position; insert `;` or a newline before it.

## Examples

This example uses the reserved words `for`, `in`, `do`, and `done` in a [for loop](../commands/loops.md#for-loops):

```shell
$ for i in 1 2 3; do echo $i; done
1
2
3
```

In the following example, `{`, `do`, and `}` are not reserved words because they are not the first word of the command:

```shell
$ echo { do re mi }
{ do re mi }
```

Reserved words are recognized only when they appear as a whole word. In this example, `{` and `}` are not reserved words because they are part of `{echo` and `Hello}`:

```shell
$ {echo Hello}
error: cannot execute external utility "{echo"
 --> <stdin>:1:1
  |
1 | {echo Hello}
  | ^^^^^ utility "{echo" not found
```

To use `{` and `}` as reserved words, write them as separate words:

```shell
$ { echo Hello; }
Hello
```

Per the extension described above, a reserved word is also recognized right after a subshell or a redirection. Here, the `}` closes the grouping even though it immediately follows the subshell `( … )`:

```shell
$ { ( echo Hello ) }
Hello
```

This is not portable; inserting a separator, as in `{ ( echo Hello ); }`, makes it portable.
