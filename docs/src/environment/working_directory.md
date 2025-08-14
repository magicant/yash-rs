# Working directory

The **working directory** is the directory from which relative paths are resolved. It is a property of the [shell environment](index.html) and is inherited by subshells and external utilities.

Many commands use the working directory as the default location for file operations. For example, specifying a relative path in a [redirection](../language/redirections/index.html) creates the file in the working directory. The `ls` utility lists files in the working directory if no operand is given.

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ echo "Hello, world!" > hello.txt
$ ls
hello.txt
```

## Viewing the current working directory

The `PWD` and `OLDPWD` [variables](../language/parameters/variables.md) hold the absolute pathnames of the current and previous working directories, respectively. These variables are updated automatically when the working directory [changes](#changing-the-working-directory). If you modify or unset them manually, automatic updates are no longer guaranteed.

```shell,no_run
$ echo "$PWD"
/home/user
```

You can also use the [`pwd` built-in](../builtins/pwd.md) to print the current working directory.

```shell,no_run
$ pwd
/home/user
```

## Changing the working directory

Use the [`cd` built-in](../builtins/cd.md) to change the working directory.

```shell
$ cd /tmp
$ pwd
/tmp
$ cd /dev
$ echo "$PWD" "$OLDPWD"
/dev /tmp
```
