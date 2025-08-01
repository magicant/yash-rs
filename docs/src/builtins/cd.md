# Cd built-in

The **`cd`** built-in changes the working directory.

## Synopsis

```sh
cd [-L|-P [-e]] [directory]
```

## Description

The built-in changes the working directory to the specified directory. The
new working directory is determined from the option and operand as follows:

1. If the operand is omitted, the value of `$HOME` is used for the operand.
   If the operand is a single hyphen (`-`), the value of `$OLDPWD` is used
   for the operand. If the variable is not set or empty, it is an error.
   Otherwise, the operand is used as is.
2. If the operand does not start with a slash (`/`) and the first pathname
   component in the operand is neither dot (`.`) nor dot-dot (`..`), the
   built-in searches the directories specified by the `$CDPATH` variable for
   a first directory that contains the operand as a subdirectory.
   If such a directory is found, the operand is replaced with the path to
   the subdirectory, that is, the concatenation of the directory contained
   in `$CDPATH` and the previous operand.
   If no such directory is found, the operand is used as is.
   (See below for security implications of `$CDPATH`.)
3. If the `-L` option is effective, the operand is canonicalized as follows:
    1. If the operand does not start with a slash (`/`), the value of `$PWD`
       is prepended to the operand.
    2. The dot (`.`) components in the operand are removed.
    3. The dot-dot (`..`) components in the operand are removed along with
       the preceding component. However, if such a preceding component
       refers to a non-existent directory, it is an error.
    4. Redundant slashes in the operand are removed.

The working directory is changed to the operand after the above processing.
If the change is successful, the value of `$PWD` is updated to the new
working directory:

- If the `-L` option is effective, the final operand value becomes the new
  value of `$PWD`.
- If the `-P` option is effective, the new `$PWD` value is recomputed in the
  same way as `pwd -P` does, so it does not include symbolic links.

The previous `$PWD` value is assigned to `$OLDPWD`.

If the new working directory is taken from `$CDPATH` or the operand is a
single hyphen (`-`), the built-in prints the new value of `$PWD` followed by
a newline to the standard output. (TODO: This printing can be enforced or
suppressed with the **`--print`** option.)

## Options

With the **`-L`** (**`--logical`**) option, the operand is resolved
logically, that is, the canonicalization is performed as above. With the
**`-P`** (**`--physical`**) option, the operand is resolved physically; the
canonicalization is skipped.
These two options are mutually exclusive. The last specified one applies if
given both. The default is `-L`.

When the `-P` option is effective, the built-in may fail to determine the
new working directory pathname to assign to `$PWD`. By default, the exit
status does not indicate the failure. If the **`-e`** (**`--ensure-pwd`**)
option is given together with the `-P` option, the built-in returns exit
status 1 in this case.

TODO: The **`--default-directory=directory`** option is not implemented.

TODO: The **`--print={always,auto,never}`** option is not implemented.

## Operands

The built-in takes a single operand that specifies the directory to change
to. If omitted, the value of `$HOME` is used. If the operand is a single
hyphen (`-`), the value of `$OLDPWD` is used.

## Errors

This built-in fails if the working directory cannot be changed, for example,
in the following cases:

- The operand does not resolve to an existing accessible directory.
- The operand is omitted and `$HOME` is not set or empty.
- The operand is a single hyphen (`-`) and `$OLDPWD` is not set or empty.
- The resolved pathname of the new working directory is too long.

It is also an error if a given operand is an empty string.

If the `-P` option is effective, the built-in may fail to determine the
new working directory pathname to assign to `$PWD`, for example, in the
following cases:

- The new pathname is too long.
- Some ancestor directories of the new working directory are not accessible.
- The new working directory does not belong to the filesystem tree.

In these cases, the working directory remains changed, the `$PWD` variable
is left empty, and the exit status depends on the `-e` option.

The built-in may also fail if `$PWD` or `$OLDPWD` is read-only. In this case,
the working directory remains changed, but the variable is not updated.

If the new working directory name cannot be printed to the standard output,
the built-in prints a warning message to the standard error, but this does
not affect the working directory change or the exit status.

## Exit Status

- If the working directory is changed successfully, the exit status is zero,
  except in the following cases where the exit status is one:
  - The `-P` and `-e` options are effective and the new working directory
    pathname cannot be determined.
  - The `$PWD` or `$OLDPWD` variable is read-only.
- If the working directory cannot be changed because of an error in the
  underlying `chdir` system call, the exit status is two.
- If the `-L` option is effective and canonicalization fails because of a
  `..` component referring to a non-existent directory, the exit status is
  three.
- If the operand cannot be processed because of an unset or empty `$HOME` or
  `$OLDPWD`, the exit status is four.
- If the command arguments are invalid, the exit status is five.

## Security considerations

Although `$CDPATH` can be helpful if used correctly, it can catch unwary
users off guard, leading to unintended changes in the behavior of shell
scripts. If a shell script is executed with the `$CDPATH` environment
variable set to a directory crafted by an attacker, the script may change
the working directory to an unexpected one. To ensure that the cd built-in
behaves as intended, shell script writers should unset the variable at the
beginning of the script. Users can configure `$CDPATH` in their shell
sessions, but should avoid exporting the variable to the environment.

## Portability

The `-L`, `-P`, and `-e` options are defined in POSIX. The other options are
non-standard.

The shell sets `$PWD` on the startup and modifies it in the cd built-in.
If `$PWD` is modified or unset otherwise, the behavior of the cd and
[pwd](crate::pwd) built-ins is unspecified.

The error handling behavior and the exit status do not agree between
existing implementations when the built-in fails because of a write error or
a read-only variable error.

Other implementations may return different non-zero exit statuses in cases
where this implementation would return exit statuses between 2 and 4.

POSIX requires the shell to convert the pathname passed to the underlying
`chdir` system call to a shorter relative pathname when the `-L` option is
in effect. This conversion is mandatory if:

- the original operand was not longer than PATH_MAX bytes (including the
  terminating nul byte),
- the final operand is longer than PATH_MAX bytes (including the terminating
  nul byte), and
- the final operand starts with `$PWD` and hence can be considered to be a
  subdirectory of the current working directory.

POSIX does not specify whether the shell should perform the conversion if
the above conditions are not met. The current implementation does it if and
only if the final operand starts with `$PWD`.
