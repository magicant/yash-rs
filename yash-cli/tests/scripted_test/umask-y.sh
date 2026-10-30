# umask-y.sh: yash-specific test of the umask built-in

test_oE -e 0 'long option name accepted as an extension'
umask 022
umask --symbolic
__IN__
u=rwx,g=rx,o=rx
__OUT__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
umask --symbolic
__IN__

test_oE -e 0 'short option name still accepted under the portable option' -o portable
umask 022
umask -S
__IN__
u=rwx,g=rx,o=rx
__OUT__

test_oE -e 0 'help option prints help'
umask --help
__IN__
umask: print or set the file mode creation mask

Usage:
  umask [-S] [mode]

Options:
  -S, --symbolic  print the mask in symbolic notation
      --help      print this help

See <https://magicant.github.io/yash-rs/builtins/umask.html> for details.
__OUT__

test_O -d -e 2 'help option with another option'
umask --help -S
__IN__
