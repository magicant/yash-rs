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
