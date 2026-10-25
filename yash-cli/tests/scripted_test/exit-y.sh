# exit-y.sh: yash-specific test of the exit built-in

test_OE -e 3 'long option name accepted as an extension'
exit --force 3
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
exit --force 3
__IN__

test_O -d -e 2 'short option name also rejected under the portable option' -o portable
exit -f 3
__IN__

test_oE -e 0 'help option prints help without exiting the shell'
exit --help
echo $?
__IN__
exit: exit the shell

Usage:
  exit [-f] [exit_status]

Options:
  -f, --force  exit even if there are suspended jobs
      --help   print this help

See <https://magicant.github.io/yash-rs/builtins/exit.html> for details.
0
__OUT__

test_O -d -e 2 'help option with another option'
exit --help -f
__IN__
