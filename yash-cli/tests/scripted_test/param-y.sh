# param-y.sh: yash-specific test of parameter expansion

test_O -d -e 2 'portable option rejects length modifier on $*' -o portable
echo ${#*}
__IN__

test_O -d -e 2 'portable option rejects length modifier on $@' -o portable
echo ${#@}
__IN__

test_O -d -e 2 'portable option rejects switch modifier on $*' -o portable
echo ${*+x}
__IN__

test_O -d -e 2 'portable option rejects switch modifier on $@' -o portable
echo ${@:+x}
__IN__

test_O -d -e 2 'portable option rejects trim modifier on $#' -o portable
echo ${#%x}
__IN__

test_O -d -e 2 'portable option rejects prefix trim modifier on $#' -o portable
echo ${##x}
__IN__

test_O -d -e 2 'portable option rejects trim modifier on $*' -o portable
echo ${*#x}
__IN__

test_O -d -e 2 'portable option rejects trim modifier on $@' -o portable
echo ${@%x}
__IN__

test_O -e 0 'without portable, unspecified parameter modifiers are accepted'
set -- ax bx
: ${#*}
: ${#@}
: ${*+x}
: ${@:+x}
: ${#%x}
: ${##x}
: ${###x}
: ${*#x}
: ${@%x}
__IN__
