# Field splitting

**Field splitting** breaks a word into fields at delimiters. This happens after [parameter expansion], [command substitution], and [arithmetic expansion], but before [pathname expansion] and quote removal.

In this example, `$flags` is split at the space, so `ls` receives two arguments:

```shell,no_run
$ flags='-a -l'
$ ls $flags
total 10468
drwxr-xr-x  2 user group    4096 Oct 10 12:34 Documents
drwxr-xr-x  3 user group    4096 Oct 10 12:34 Downloads
drwxr-xr-x  3 user group    4096 Oct 10 12:34 Music
drwxr-xr-x  2 user group    4096 Oct 10 12:34 Pictures
drwxr-xr-x  2 user group    4096 Oct 10 12:34 Videos
```

Field splitting does not occur if the expansion is [quoted](quoting.md):

```shell,no_run
$ flags='-a -l'
$ ls "$flags"
ls: invalid option -- ' '
Try 'ls --help' for more information.
```

Field splitting only applies to the results of [parameter expansion], [command substitution], and [arithmetic expansion], not to literals or [tilde expansions](tilde.md):

```shell,no_run
$ HOME='/home/user/My Documents'
$ ls ~
Documents  Downloads  Music  Pictures  Videos
$ ls "$HOME"
Documents  Downloads  Music  Pictures  Videos
$ ls $HOME
ls: cannot access '/home/user/My': No such file or directory
ls: cannot access 'Documents': No such file or directory
```

Field splitting only happens where words are expected, such as simple command words, for loop words, and array assignments. It does not occur in contexts expecting a single word, like scalar assignments or case patterns.

```shell,no_run
$ flags='-a -l'
$ oldflags=$flags # no field splitting; oldflags is '-a -l'
$ flags="$flags -r"
$ ls $flags # field splitting; ls receives '-a', '-l', and '-r'
Videos  Pictures  Music  Downloads  Documents
$ flags=$oldflags # again, no field splitting
$ echo "Restored flags: $flags"
Restored flags: -a -l
```

## IFS

Field splitting is controlled by the `IFS` (Internal Field Separator) variable, which lists delimiter characters. By default, `IFS` contains a space, tab, and newline. You can change `IFS` to use different delimiters.

If `IFS` is unset, the default value is used:

```shell,no_run
$ unset IFS
$ flags='-a -l'
$ ls $flags
total 10468
drwxr-xr-x  2 user group    4096 Oct 10 12:34 Documents
drwxr-xr-x  3 user group    4096 Oct 10 12:34 Downloads
drwxr-xr-x  3 user group    4096 Oct 10 12:34 Music
drwxr-xr-x  2 user group    4096 Oct 10 12:34 Pictures
drwxr-xr-x  2 user group    4096 Oct 10 12:34 Videos
```

If `IFS` is set to an empty string, no splitting occurs:

```shell,no_run
$ IFS=''
$ flags='-a -l'
$ ls $flags
ls: invalid option -- ' '
Try 'ls --help' for more information.
```

Each character in `IFS` is a delimiter. How fields are split depends on whether a delimiter is whitespace or not.

Non-whitespace delimiters split the word at their position and may produce empty fields:

```shell
$ IFS=':'
$ values='a:b::c:d'
$ for value in $values; do echo "[$value]"; done
[a]
[b]
[]
[c]
[d]
```

Empty fields are not produced after a trailing non-whitespace delimiter:

```shell
$ IFS=':'
$ values='a:b:'
$ for value in $values; do echo "[$value]"; done
[a]
[b]
```

Whitespace delimiters also split the word, but do not produce empty fields. Multiple whitespace delimiters in a row are treated as one:

```shell
$ IFS=' '
$ values=' a  b   c'
$ for value in $values; do echo "[$value]"; done
[a]
[b]
[c]
```

Whitespace and non-whitespace delimiters can be combined in `IFS`:

```shell
$ IFS=' :'
$ values='a:b  c : d:  :e  f '
$ for value in $values; do echo "[$value]"; done
[a]
[b]
[c]
[d]
[]
[e]
[f]
```

<p class="warning">
Currently, yash only supports UTF-8 encoded text. This does not fully conform to POSIX, which requires handling arbitrary byte sequences.
</p>

## Empty field removal

During field splitting, empty fields are removed, except those delimited by non-whitespace `IFS` characters.

```shell
$ empty='' space=' '
$ for value in $empty; do echo "[$value]"; done # prints nothing
$ for value in $space; do echo "[$value]"; done # prints nothing
```

Empty fields are removed even if `IFS` is empty:

```shell
$ IFS=''
$ empty='' space=' '
$ for value in $empty; do echo "[$value]"; done # prints nothing
$ for value in $space; do echo "[$value]"; done # prints one field containing a space
[ ]
```

To retain empty fields, quote the word to prevent field splitting:

```shell
$ empty='' space=' '
$ for value in "$empty"; do echo "[$value]"; done
[]
$ for value in "$space"; do echo "[$value]"; done
[ ]
```

[parameter expansion]: parameters.md
[command substitution]: command_substitution.md
[arithmetic expansion]: arithmetic.md
[pathname expansion]: globbing.md
