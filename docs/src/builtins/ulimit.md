# Ulimit built-in

The **`ulimit`** built-in displays or sets system resource limits for the current shell process.

## Synopsis

```sh
ulimit [-SH] [-a|-b|-c|-d|-e|-f|-i|-k|-l|-m|-n|-q|-R|-r|-s|-t|-u|-v|-w|-x] [limit]
```

## Description

The `ulimit` built-in allows you to view or change resource limits imposed by the operating system on the shell and its child processes. The *limit* operand specifies the new value for a resource limit. If *limit* is omitted, the current value is displayed.

Each resource has two limits:

- **Soft limit**: The value enforced by the kernel for the process. It can be increased up to the hard limit.
- **Hard limit**: The maximum value to which the soft limit can be set. Any process can lower its hard limit, but only privileged processes can raise it.

## Options

- **`-S`** (**`--soft`**): Set or display the soft limit.
- **`-H`** (**`--hard`**): Set or display the hard limit.

If neither `-S` nor `-H` is specified:

- When setting a limit, both soft and hard limits are changed.
- When displaying a limit, only the soft limit is shown.

Specifying both `-S` and `-H` together sets both limits. However, when displaying limits, using both options is an error.

### Resource selection

Specify a resource to set or display using one of the following options. Supported resources may vary by platform, so not all options are available everywhere:

- **`-b`** (**`--sbsize`**): maximum size of the socket buffer (bytes)
- **`-c`** (**`--core`**): maximum size of a core file created by a terminated process (512-byte blocks)
- **`-d`** (**`--data`**): maximum size of a data segment of the process (kilobytes)
- **`-e`** (**`--nice`**): maximum process priority (see below)
- **`-f`** (**`--fsize`**): maximum size of a file the process can create (512-byte blocks)
- **`-i`** (**`--sigpending`**): maximum number of signals that can be queued to the process
- **`-k`** (**`--kqueues`**): maximum number of kernel event queues
- **`-l`** (**`--memlock`**): maximum size of memory locked into RAM (kilobytes)
- **`-m`** (**`--rss`**): maximum physical memory size of the process (kilobytes)
- **`-n`** (**`--nofile`**): maximum number of open files in the process
- **`-q`** (**`--msgqueue`**): maximum total size of POSIX message queues
- **`-R`** (**`--rttime`**): maximum amount of CPU time the process can consume in real-time scheduling mode without a blocking system call (microseconds)
- **`-r`** (**`--rtprio`**): maximum real-time priority
- **`-s`** (**`--stack`**): maximum size of the process's stack (kilobytes)
- **`-t`** (**`--cpu`**): maximum amount of CPU time the process can consume (seconds)
- **`-u`** (**`--nproc`**): maximum number of processes the user can run
- **`-v`** (**`--as`**): maximum total memory size of the process (kilobytes)
- **`-w`** (**`--swap`**): maximum size of the swap space the user can occupy (kilobytes)
- **`-x`** (**`--locks`**): maximum number of file locks the process can hold

The *limit* operand and output values use the units shown in parentheses above.

If no resource option is specified, `ulimit` defaults to `-f` (`--fsize`).

For `-e` (`--nice`), the limit value sets the lowest nice value allowed, using the formula: `nice = 20 - limit`. Lower nice values mean higher priority. For example, `ulimit -e 25` allows the nice value to be lowered to -5.

To display all resource limits, use **`-a`** (**`--all`**). This cannot be combined with a *limit* operand.

## Operands

The *limit* operand sets a new value for the selected resource. It is interpreted as follows:

- A non-negative integer sets the limit to that value.
- `unlimited` sets the limit to the maximum allowed.
- `hard` sets the limit to the current hard limit.
- `soft` sets the limit to the current soft limit.

## Standard output

If *limit* is omitted, the built-in prints the current value for the selected resource. With `-a`, it prints all resource limits in a table.

## Errors

The built-in fails if:

- The specified resource is unsupported on the current platform.
- The soft limit is set higher than the hard limit.
- The hard limit is set above the current hard limit without sufficient privileges.
- The *limit* operand is out of range.
- Both `-S` and `-H` are specified without a *limit* operand.
- More than one resource option is specified.

## Exit status

Zero if successful; non-zero if an error occurs.

## Examples

Setting resource limits:

```shell,hidelines=#
$ ulimit -n 64
$ ulimit -t unlimited
$ ulimit -S -v hard
#$ ulimit -d hard
$ ulimit -H -d soft
```

Showing resource limits:

```shell,hidelines=#
#$ ulimit -n 64
#$ ulimit -S -n 32
$ ulimit -H -n
64
$ ulimit -S -n
32
$ ulimit -n
32
```

Showing all resource limits:

```shell,no_run
$ ulimit -a
-v: virtual address space size (KiB) unlimited
-c: core dump size (512-byte blocks) 0
-t: CPU time (seconds)               unlimited
-d: data segment size (KiB)          unlimited
-f: file size (512-byte blocks)      unlimited
-x: number of file locks             unlimited
-l: locked memory size (KiB)         65536
-q: message queue size (bytes)       819200
-e: process priority (20 - nice)     0
-n: number of open files             1024
-u: number of processes              62113
-m: resident set size (KiB)          unlimited
-r: real-time priority               0
-R: real-time timeout (microseconds) unlimited
-i: number of pending signals        62113
-s: stack size (KiB)                 8192
```

## Compatibility

The `ulimit` built-in is specified by POSIX.1-2024, but some behaviors are implementation-defined.

Only these options are required by POSIX: `-H`, `-S`, `-a`, `-c`, `-d`, `-f`, `-n`, `-s`, `-t`, and `-v`. Other options are extensions.

Some shells do not allow combining options (e.g., `ulimit -fH`). For portability, specify options separately (e.g., `ulimit -f -H`).

Shells differ in behavior when both `-H` and `-S` are given. Yash-rs sets or displays both limits; older versions of yash only honored the last one.

Specifying multiple resource options is an error in yash-rs, but some shells
allow operating on multiple resources at once.

The `hard` and `soft` values for the *limit* operand are not defined by POSIX.
