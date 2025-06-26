# Here-documents

**Here-documents** are a type of [redirection](index.html) that lets you provide multi-line input directly within a script or command line. They are useful for supplying input to commands or scripts without creating a separate file.

## Syntax

A here-document starts with the `<<` operator followed by a **delimiter** word. After the next newline [operator](../words/index.html#tokens-and-operators), the shell reads lines until it finds a line containing only the delimiter (with no trailing blanks). The lines read become the standard input for the command.

```shell
$ cat <<EOF
> Hello,
> World!
> EOF
Hello,
World!
```

In this example, `EOF` is the delimiter. The `cat` utility receives the lines between `<<EOF` and the final `EOF`.

POSIX allows any word as a delimiter, but for portability, use only alphanumeric characters and underscores. Delimiters with special characters or whitespace can cause unexpected behavior, especially if not quoted. See also [Quoting the delimiter and expanding the content](#quoting-the-delimiter-and-expanding-the-content) below for the effects of quoting the delimiter.

### Multiple here-documents

You can use multiple here-document operators in a single command or across multiple commands on the same line. After the next newline, the shell reads lines for each here-document in order, stopping at each delimiter.

```shell
$ cat <<EOF; cat <<END <<EOF
> Hello,
> EOF
> This is the first here-document for the second command.
> END
> World!
> EOF
Hello,
World!
```

### Here-documents in command substitution

When using a here-document inside command substitution, the content must be included within the substitution syntax:

```shell
$ echo $(cat <<EOF
> Hello,
> World!
> EOF
> )
Hello, World!
```

It is not supported to place the here-document content outside the command substitution, as in:

```sh
echo $(cat <<EOF)
Hello,
World!
EOF
```

## Automatic removal of leading tabs

If you use `<<-` instead of `<<`, all leading tab characters are removed from the here-document content and the delimiter line. This allows you to indent here-documents in your scripts for readability, without affecting the output.

<!-- markdownlint-disable MD010 -->
```shell
$ cat <<-EOF
> 		Hello,
> 		World!
> 	EOF
Hello,
World!
```
<!-- markdownlint-enable MD010 -->

Note: Only leading tabs are removed, not spaces.

## Quoting the delimiter and expanding the content

If the delimiter after the redirection operator is [quoted](../words/quoting.md), [quote removal](../words/quoting.md#quote-removal) is performed on the delimiter, and the result is used to find the end of the here-document. In this case, the content is not subject to any expansions and is treated literally.

```shell
$ user="Alice"
$ cat <<'EOF'
> Hello, $user!
> 1 + 1 = $((1 + 1)).
> EOF
Hello, $user!
1 + 1 = $((1 + 1)).
```

If the delimiter is not quoted, the following are handled in the here-document content when the redirection is performed:

- [Backslash escapes](../words/quoting.md#backslash) work only before `$`, `` ` ``, and `\`. Other backslashes are literal.
- [Line continuations](../words/quoting.md#line-continuation) are removed.
- [Parameter expansion](../words/parameters.md), [command substitution](../words/command_substitution.md), and [arithmetic expansion](../words/arithmetic.md) are performed.

Single and double quotes in the here-document content are treated literally.

```shell
$ user="Alice"
$ cat <<EOF
> Hello, $user!
> 1 + 1 = $((1 + 1)).
> EOF
Hello, Alice!
1 + 1 = 2.
```
