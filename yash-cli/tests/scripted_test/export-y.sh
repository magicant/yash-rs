# export-y.sh: yash-specific test of the export built-in

test_O -d -e n 'export rejects non-portable variable name' -o portable
export foo-bar=1
echo not reached
__IN__

test_O -d -e n 'export rejects variable name starting with a digit' -o portable
export 1abc=1
echo not reached
__IN__

test_OE -e 0 'export accepts non-portable variable name without the portable option'
export foo-bar=1
__IN__

test_O -d -e n 'export rejects missing operand' -o portable
export
echo not reached
__IN__

test_O -d -e n 'export rejects operand with the -p option' -o portable
export foo=bar
export -p foo
echo not reached
__IN__

test_oE 'export error message mentions the portable option' -o portable
(export) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_oE 'export error message names the unexpected operand' -o portable
export foo=bar
(export -p foo) 2>result
grep -Fq foo result && echo shown
__IN__
shown
__OUT__

test_oE -e 0 'export accepts the -p option without operands' -o portable
export foo=bar
export -p | grep -Fx 'export foo=bar'
__IN__
export foo=bar
__OUT__

test_oE -e 0 'export accepts operands without the -p option' -o portable
export foo=bar
echo "$foo"
__IN__
bar
__OUT__

test_oE -e 0 'export accepts missing operand without the portable option'
export foo=bar
export | grep -Fx 'export foo=bar'
__IN__
export foo=bar
__OUT__

test_oE -e 0 'export accepts operand with the -p option without the portable option'
export foo=bar
export -p foo
__IN__
export foo=bar
__OUT__

test_O -d -e n 'export rejects long option name' -o portable
export --print
echo not reached
__IN__

test_O -d -e n 'export rejects abbreviated long option name' -o portable
export --p
echo not reached
__IN__

test_oE 'export long option error message mentions the portable option' -o portable
(export --print) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_oE -e 0 'export accepts long option name without the portable option'
export foo=bar
export --print foo
__IN__
export foo=bar
__OUT__

test_oE 'export ++print error message does not blame the portable option' -o portable
(export ++print) 2>result
grep -Fq portable result || echo not shown
__IN__
not shown
__OUT__

test_oE -e 0 'help option prints help without exiting the shell'
export --help
echo $?
__IN__
export: export variables to the environment

Usage:
  export [-p] [NAME[=VALUE]...]

Options:
  -p, --print  print the current definitions
      --help   print this help

See <https://magicant.github.io/yash-rs/builtins/export.html> for details.
0
__OUT__

test_OE -e 0 'help built-in prints the same help as the help option'
export --help >help_option
help export >help_builtin
diff help_option help_builtin
__IN__

test_OE -e 0 'help option can be abbreviated and followed by a separator'
export --he >/dev/null &&
export --help -- >/dev/null
__IN__

test_O -d -e 2 'help option with another option'
export --help -p
__IN__

test_O -d -e 2 'help option with an operand'
export --help foo
__IN__

test_O -d -e 2 'help option with an operand after a separator'
export --help -- foo
__IN__

test_O -d -e 2 'help option cannot be negated'
export ++help
__IN__
