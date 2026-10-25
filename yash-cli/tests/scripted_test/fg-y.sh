# fg-y.sh: yash-specific test of the fg built-in

test_O -d -e 2 'lone % rejected as an operand under the portable option' -m -o portable
sh -c 'kill -s STOP $$'
fg %
__IN__

test_O -d -e 2 'operand without the leading % rejected as a syntax error' -m
sh -c 'kill -s STOP $$'
fg 1
__IN__

test_oE -e 0 'help option prints help'
fg --help
__IN__
fg: resume a job in the foreground

Usage:
  fg [job_id]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/fg.html> for details.
__OUT__

test_O -d -e 2 'help option with another operand'
fg --help %1
__IN__
