# return-y.sh: yash-specific test of the return built-in

test_OE -e 3 'long option name accepted as an extension'
f() { return --no-return 3; }
f
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
f() { return --no-return 3; }
f
__IN__

test_O -d -e 2 'short option name also rejected under the portable option' -o portable
f() { return -n 3; }
f
__IN__

test_oE -e 0 'help option prints help without exiting the shell'
return --help
echo $?
__IN__
return: return from a function or script

Usage:
  return [-n] [exit_status]

Options:
  -n, --no-return  set the exit status without returning
      --help       print this help

See <https://magicant.github.io/yash-rs/builtins/return.html> for details.
0
__OUT__

test_O -d -e 2 'help option with another option'
return --help -n
__IN__
