# break-y.sh: yash-specific test of the break built-in

test_oE -e 0 'help option prints help without exiting the shell'
break --help
echo $?
__IN__
break: exit from loops

Usage:
  break [N]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/break.html> for details.
0
__OUT__

test_O -d -e 2 'help option with another operand'
break --help 1
__IN__
