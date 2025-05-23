# Reserved words

Some words have special meaning in shell syntax. These **reserved words** must be [quoted](quoting.md) to use them literally. The reserved words are:

- `!` – Negation
- `{` – Start of a grouping
- `}` – End of a grouping
- `[[` – Start of a double bracket command
- `case` – Case command
- `do` – Start of a loop or conditional block
- `done` – End of a loop or conditional block
- `elif` – Else if clause
- `else` – Else clause
- `esac` – End of a case command
- `fi` – End of an if command
- `for` – For loop
- `function` – Function definition
- `if` – If command
- `in` – Delimiter for a for loop
- `then` – Then clause
- `until` – Until loop
- `while` – While loop

Currently, `[[` and `function` are only recognized as reserved words; their functionality is not yet implemented.

Additionally, the POSIX standard allows for the following optional reserved words:

- `]]` – End of a double bracket command
- `namespace` – Namespace declaration
- `select` – Select command
- `time` – Time command

These four words are not reserved in yash now, but may be in the future.

## Where are reserved words recognized?

Reserved words are recognized in these contexts:

- As the first word of a command
- As a word following any reserved word other than `case`, `for`, or `in`
- `in` as the third word in a for loop or `case` command
- `do` as the third word in a for loop

## Examples

This example uses the reserved words `for`, `in`, `do`, and `done` in a for loop:

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
