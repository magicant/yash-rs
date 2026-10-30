# times-y.sh: yash-specific test of the times built-in

test_oE -e 0 'help option prints help without exiting the shell'
times --help
echo $?
__IN__
times: print accumulated user and system times

Usage:
  times

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/times.html> for details.
0
__OUT__

test_O -d -e 2 'help option with another operand'
times --help foo
__IN__
