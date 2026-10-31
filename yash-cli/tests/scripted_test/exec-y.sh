# exec-y.sh: yash-specific test of the exec built-in

test_oE -e 0 'help option prints help without exiting the shell'
exec --help
echo $?
__IN__
exec: replace the shell process with a utility

Usage:
  exec [NAME [ARGUMENT...]]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/exec.html> for details.
0
__OUT__

test_O -d -e 2 'help option with another operand'
exec --help echo foo
__IN__
