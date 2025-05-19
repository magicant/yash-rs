# Parameters

A **parameter** is a name-value pair used to store and retrieve data in a shell script. Parameters can be variables, special parameters, or positional parameters.

Parameter expansion retrieves the value of a parameter when the command is executed.

```shell
$ name="Alice" # define a variable
$ echo "Hello, $name!" # expand the variable
Hello, Alice!
```
