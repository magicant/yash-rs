# Reserved words

Some words have special meaning in shell syntax. These **reserved words** must be [quoted](quoting.md) to use them literally. The reserved words are:

- `!` – [Negation](../commands/pipelines.md#negation)
- `{` – Start of a [grouping](../commands/grouping.md#braces)
- `}` – End of a [grouping](../commands/grouping.md#braces)
- `[[` – Start of a double bracket command
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
- `then` – Then clause
- `until` – [Until loop](../commands/loops.md#while-and-until-loops)
- `while` – [While loop](../commands/loops.md#while-and-until-loops)

Currently, `[[` and `function` are only recognized as reserved words; their functionality is not yet implemented.

Additionally, the POSIX standard allows for the following optional reserved words:

- `]]` – End of a double bracket command
- `namespace` – Namespace declaration
- `select` – Select command
- `time` – Time command

These four words are not reserved in yash-rs now, but may be in the future.

## Where are reserved words recognized?

Reserved words are recognized in these contexts:

- As the first word of a [command](../commands/index.html#commands-1)
- As a word following any reserved word other than `case`, `for`, or `in`
- `in` as the third word in a [for loop](../commands/loops.md#for-loops) or [case command](../commands/case.md)
- `do` as the third word in a [for loop](../commands/loops.md#for-loops)

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
  | ^^^^^ utility not found
  |
```

To use `{` and `}` as reserved words, write them as separate words:

```shell
$ { echo Hello; }
Hello
```
