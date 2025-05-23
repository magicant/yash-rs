# Getting started

## Starting the shell

To start the shell, run `yash3` from the command line. This starts an interactive shell session.

```sh
yash3
```

You will see a prompt, indicating the shell is ready for commands:

```shell,ignore
$
```

## Using the shell interactively

Once the shell is running, you can type commands. The shell executes each command you enter, and you will see the output in the terminal.

Most commands run a utility, which is a program that performs a specific task. For example, you can run the `echo` utility to print a message:

```shell
$ echo "Hello, world!"
Hello, world!
```

In this example, `$` is the shell prompt, and `echo "Hello, world!"` is the command you entered. The shell executed the `echo` utility, which printed "Hello, world!" to the terminal.

You can also run other utilities, such as `ls`, which lists the files in the working directory:

```shell,no_run
$ ls
Documents  Downloads  Music  Pictures  Videos
```

The output varies depending on the files in your working directory.

## Interrupting a command

To interrupt a running command, press `Ctrl+C`. This sends an interrupt signal to the running utility, causing it to terminate. For example, if you run a command that takes a long time, you can cancel it with `Ctrl+C`:

```shell,no_run
$ sleep 10
```

This command sleeps for 10 seconds, but you can interrupt it by pressing `Ctrl+C`. This aborts the `sleep` utility and returns you to the shell prompt immediately.

Note: Some utilities may not respond to `Ctrl+C` if they are designed to ignore or handle the interrupt signal differently.

## Exiting the shell

To exit the shell, use the `exit` command:

```shell
$ exit
```

This ends the shell session and returns you to your previous shell.

Alternatively, you can press `Ctrl+D` to exit the shell. This sends an empty command to the shell, causing it to exit.

## Running scripts

You can also run scripts in the shell. To do this, create a script file with the commands you want to run. For example, create a file called `script.sh` with the following content:

```sh
echo "This is a script"
echo "Running in the shell"
```

Run this script in the shell by using the `.` utility:

```shell,no_run
$ . ./script.sh
This is a script
Running in the shell
```

You can also run the script by passing it as an argument to the shell:

```shell,no_run
$ yash3 ./script.sh
This is a script
Running in the shell
```

This runs the script in a new shell session. The output will be the same.

If you make the script executable, you can run it directly:

```shell,no_run
$ chmod a+x script.sh
$ ./script.sh
This is a script
Running in the shell
```

The `chmod` utility makes the script file executable. This allows you to run the script directly, without specifying the shell explicitly, as in the previous example.

Note the `./` in the commands above. This indicates that the script is in the current directory. If you omit `./`, the shell searches for the script in the directories listed in the `PATH` environment variable. If the script is not in one of those directories, you will get a "utility not found" error.
