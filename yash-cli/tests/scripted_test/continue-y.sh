# continue-y.sh: yash-specific test of the continue built-in

test_oE -e 0 'help option prints help without exiting the shell'
continue --help
echo $?
__IN__
continue: resume the next iteration of loops

Usage:
  continue [n]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/continue.html> for details.
0
__OUT__

test_O -d -e 2 'help option with another operand'
continue --help 1
__IN__
