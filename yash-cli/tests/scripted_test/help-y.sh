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
