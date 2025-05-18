# Words, tokens, and fields

In the shell language, a **word** is a sequence of characters, usually separated by whitespace. Words represent commands, arguments, and other elements in the shell.

In this example, `echo`, `Hello,`, and `world!` are all words:

```shell
$ echo Hello, world!
Hello, world!
```

The first word (`echo`) is the name of the utility to run. The other words are arguments passed to that utility.

Before running the utility, the shell expands words. This means the shell processes certain characters and sequences in the words to produce the final command line. For example, `$` is used for parameter expansion, letting you access variable values:

```shell
$ name="Alice"
$ echo Hello, $name!
Hello, Alice!
```

Here, `$name` is expanded to its value (`Alice`) before `echo` runs.

To prevent expansion, [quote](quoting.md) the characters you want to treat literally. For example, to print `$name` without expanding it, use single quotes:

```shell
$ echo '$name'
$name
```

## Tokens and operators

A **token** is a sequence of characters processed as a single unit in shell syntax. The shell divides input into tokens, which are then parsed to form commands. A token is either a word or an operator.

The shell recognizes these **operators**:

- Newline – Command separator
- `;` – Command separator
- `&` – Asynchronous command
- `&&` – Logical and
- `||` – Logical or
- `|` – Pipeline
- `(` – Start of a subshell
- `)` – End of a subshell
- `;;` – End of a case item
- `;&` – End of a case item
- `;;&` – End of a case item
- `;|` – End of a case item
- `<` – Input redirection
- `<&` – Input redirection
- `<(` – Process redirection
- `<<` – Here document
- `<<-` – Here document
- `<<<` – Here string
- `<>` – Input and output redirection
- `>` – Output redirection
- `>&` – Output redirection
- `>|` – Output redirection
- `>(` – Process redirection
- `>>` – Output redirection
- `>>|` – Pipeline redirection

When recognizing operators, the shell matches the longest possible sequence first. For example, `&&` is a single operator, not two `&` operators, and `<<<<` is recognized as `<<<` and `<`, not two `<<` operators.

Blank characters (spaces and tabs) separate tokens unless [quoted](quoting.md). Words (non-operator tokens) must be separated by at least one blank character. Operators do not need to be separated by blanks if they are recognized as expected.

These two lines are equivalent:

```shell
$ ((echo hello))
hello
$ ( ( echo hello ) )
hello
```

However, you cannot omit the space between `;` and `;;` in a case command:

```shell
$ case foo in (foo) echo foo; ;; esac
foo
$ case foo in (foo) echo foo;;; esac
error: the pattern is not a valid word token
 --> <stdin>:2:29
  |
2 | case foo in (foo) echo foo;;; esac
  |                             ^ expected a word
  |
```

## Word expansion

The shell performs several types of word expansion. The following expansions happen first:

- Tilde expansion
- Parameter expansion
- Command substitution
- Arithmetic expansion

After these, the shell performs these steps in order:

<!-- Brace expansion is not yet implemented. -->
1. Field splitting
2. Pathname expansion
3. Quote removal

The result is a list of words passed to the utility. Each word resulting from these expansions is called a **field**.
