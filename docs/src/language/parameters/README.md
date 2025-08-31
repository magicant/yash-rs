# Parameters

A **parameter** is a name-value pair used to store and retrieve data in a shell script. Parameters can be [variables](variables.md), [special parameters](special.md), or [positional parameters](positional.md).

[Parameter expansion](../words/parameters.md) retrieves the value of a parameter when the command is executed.

```shell
$ name="Alice" # define a variable
$ echo "Hello, $name!" # expand the variable
Hello, Alice!
```
