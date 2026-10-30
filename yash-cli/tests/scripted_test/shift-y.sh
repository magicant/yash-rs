# shift-y.sh: yash-specific test of the shift built-in

test_oE -e 0 'help option prints help without exiting the shell'
shift --help
echo $?
__IN__
shift: remove positional parameters

Usage:
  shift [N]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/shift.html> for details.
0
__OUT__

test_O -d -e 2 'help option with another operand'
shift --help 1
__IN__
