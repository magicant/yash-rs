# eval-y.sh: yash-specific test of the eval built-in

test_oE -e 0 'help option prints help without exiting the shell'
eval --help
echo $?
__IN__
eval: evaluate arguments as shell commands

Usage:
  eval [command...]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/eval.html> for details.
0
__OUT__

test_O -d -e 2 'help option with another operand'
eval --help echo foo
__IN__
