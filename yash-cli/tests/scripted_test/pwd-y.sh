# pwd-y.sh: yash-specific test of the pwd built-in

test_OE -e 0 'long option name accepted as an extension'
pwd --physical >/dev/null
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
pwd --physical >/dev/null
__IN__

test_OE -e 0 'short option name still accepted under the portable option' -o portable
pwd -P >/dev/null
__IN__

test_oE -e 0 'help option prints help'
pwd --help
__IN__
pwd: print the working directory path

Usage:
  pwd [-L|-P]

Options:
  -L, --logical   print $PWD if it is correct (default)
  -P, --physical  print the path without symbolic links
      --help      print this help

See <https://magicant.github.io/yash-rs/builtins/pwd.html> for details.
__OUT__

test_O -d -e 2 'help option with another option'
pwd --help -P
__IN__

test_O -d -e 2 'help option rejected under the portable option' -o portable
pwd --help
__IN__

test_O -d -e n 'help option with unwritable standard output'
pwd --help >&-
__IN__
