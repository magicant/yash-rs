# help-y.sh: yash-specific test of the help built-in

test_oE -e 0 'printing help of help'
help help
__IN__
help: print the help of built-ins

Usage:
  help [name...]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/help.html> for details.
__OUT__

test_oE -e 0 'printing help of multiple built-ins'
help help pwd
__IN__
help: print the help of built-ins

Usage:
  help [name...]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/help.html> for details.

pwd: print the working directory path

Usage:
  pwd [-L|-P]

Options:
  -L, --logical   print $PWD if it is correct (default)
  -P, --physical  print the path without symbolic links
      --help      print this help

See <https://magicant.github.io/yash-rs/builtins/pwd.html> for details.
__OUT__

test_oE -e 0 'help option prints help of help'
help --help
__IN__
help: print the help of built-ins

Usage:
  help [name...]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/help.html> for details.
__OUT__

test_o -d -e 1 'operand without help'
help help no_such_builtin
__IN__
help: print the help of built-ins

Usage:
  help [name...]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/help.html> for details.
__OUT__

test_oE -e 0 'printing help of built-ins that do not accept --help'
help : true false
__IN__
:: do nothing

Usage:
  : [argument...]

See <https://magicant.github.io/yash-rs/builtins/colon.html> for details.

true: do nothing, successfully

Usage:
  true

See <https://magicant.github.io/yash-rs/builtins/true.html> for details.

false: do nothing, unsuccessfully

Usage:
  false

See <https://magicant.github.io/yash-rs/builtins/false.html> for details.
__OUT__

test_OE -e 0 'colon built-in ignores --help'
: --help
__IN__

test_OE -e 0 'true built-in ignores --help'
true --help
__IN__

test_OE -e 1 'false built-in ignores --help'
false --help
__IN__
