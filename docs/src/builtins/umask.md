# Umask built-in

The **`umask`** built-in shows or sets the file mode creation mask.

## Synopsis

```sh
umask [-S] [mode]
```

## Description

The built-in shows the current file mode creation mask if no *mode* is
given. Otherwise, it sets the file mode creation mask to *mode*.

The **file mode creation mask** is a set of permissions that determines the default permissions for newly created files. In the numeric form, it is represented as a three-digit octal number, where each digit represents the permissions for the user, group, and others, respectively. For example, if the mask is set to `077`, all permissions for group and others are removed, and only the user can have read, write, and execute permissions.

```shell,no_run
$ umask 077
$ mkdir foo
$ echo "Hello, world!" > greet.txt
$ ls -do foo greet.txt
drwx------ 2 user 4096 Aug 11 13:07 foo
-rw------- 1 user   14 Aug 11 13:07 greet.txt
```

If the mask is `000`, all permissions are granted to the user, group, and others.

```shell,no_run
$ umask 000
$ mkdir foo
$ echo "Hello, world!" > greet.txt
$ ls -do foo greet.txt
drwxrwxrwx 2 user 4096 Aug 11 13:07 foo
-rw-rw-rw- 1 user   14 Aug 11 13:07 greet.txt
```

Note that execute permissions are not granted for the text files created by the [redirections](../language/redirections/index.html) because redirection omits the execute permission when creating files. In general, the actual permissions set for files may be more restrictive than what the current mask suggests.

## Options

The **`-S`** (**`--symbolic`**) option causes the built-in to show the current file mode creation mask in symbolic notation. Without this option, the mask is shown in octal notation.

## Operands

*mode* is an octal integer or a symbolic notation that represents the file mode creation mask. The octal number is the bitwise OR of the file mode bits to be turned off when creating a file. The symbolic notation specifies the file mode bits to be kept on when creating a file.

### Numeric notation

The numeric notation is a three-digit octal number, where each digit represents the permissions for the user, group, and others, respectively. Each digit can be a value from 0 to 7, which is a sum of:

- 4 for read permission,
- 2 for write permission,
- 1 for execute permission.

For example, the mask `027` removes write permission for the group and all permissions for others, while granting all permissions to the user.

### Symbolic notation

The symbolic notation consists of one or more clauses separated by commas. Each clause consists of a (possibly empty) sequence of who symbols followed by one or more actions. The who symbols are:

- **`u`** for the user bits,
- **`g`** for the group bits,
- **`o`** for the other bits, and
- **`a`** for all bits.

An action is an operator optionally followed by permission symbols. The
operators are:

- **`+`** to add the permission,
- **`-`** to remove the permission, and
- **`=`** to set the permission.

The permission symbols are:

- one or more of:
    - **`r`** for the read permission,
    - **`w`** for the write permission,
    - **`x`** for the execute permission,
    - **`X`** for the execute permission if the execute permission is
      already set for any who, and
    - **`s`** for the set-user-ID-on-execution and set-group-ID-on-execution
      bits.
- **`u`** for the current user permission,
- **`g`** for the current group permission, and
- **`o`** for the current other permission.

For example, the symbolic notation `u=rwx,go+r-w`:

- sets the user bits to read, write, and execute,
- adds the read permission to the group and other bits, and
- removes the write permission from the group and other bits.

A symbolic *mode* that starts with `-` may be confused as an option. To avoid this, use `umask -- -w` or `umask a-w` instead of `umask -w`.

## Standard output

If no *mode* is given, the built-in prints the current file mode creation mask in octal notation followed by a newline to the standard output. If the `-S` option is effective, the mask is formatted in symbolic notation instead.

## Errors

It is an error if the specified *mode* is not a valid file mode creation mask.

## Exit status

Zero if successful; non-zero if an error occurs.

## Examples

Setting and showing the file mode creation mask:

```shell
$ umask 027
$ umask
027
```

Using symbolic notation:

```shell
$ umask ug=rwx,g-w,o=
$ umask -S
u=rwx,g=rx,o=
```

## Security considerations

If the file mode creation mask is too permissive, sensitive files may become accessible to unauthorized users. It is generally recommended to remove write permissions for group and other users.

## Compatibility

The `umask` built-in is defined in POSIX.1-2024.

POSIX does not specify the default output format used when the `-S` option is not given. Yash-rs, as well as many others, uses octal notation. In any cases, the output can be reused as *mode* to restore the previous mask.

This implementation ignores the `-S` option if *mode* is given. However, bash prints the new mask in symbolic notation if the `-S` option and *mode* are both given.

An empty sequence of who symbols is equivalent to `a` in this implementation as well as many others. However, this may not be strictly true to the POSIX specification.

The permission symbols other than `r`, `w`, and `x` are not widely supported. This implementation currently ignores the `s` symbol.
