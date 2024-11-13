# case-y.sh: yash-specific test of case command

test_oe 'patterns separated by | are expanded and matched in order'
case 1 in
    $(echo expanded 0 >&2; echo 0) |\
    $(echo expanded 1 >&2; echo 1) |\
    $(echo expanded 2 >&2; echo 2)) echo matched;;
esac
__IN__
matched
__OUT__
expanded 0
expanded 1
__ERR__

# The behavior is unspecified in POSIX, but many existing shells seem to behave
# this way (with the notable exception of ksh).
test_OE -e 0 'exit status of case command (matched, empty)'
case $(echo 2; exit 2) in
    1) ;;
    2) ;;
    3) ;;
esac
__IN__

# The behavior is unspecified in POSIX, but many existing shells seem to behave
# this way (with the notable exception of ksh).
test_OE -e 0 'exit status of case command with ;& followed by empty item'
case i in
    i) (exit 1);&
    j) ;;
esac
__IN__

test_oE -e 42 'pattern matching after ;|'
case 1 in
    0) echo not reached 0;;
    1) echo matched 1; (exit 12);|
    2) echo not reached 2;;
    1) echo matched 2 $?; (exit 42);|
    2) echo not reached 3;;
esac
__IN__
matched 1
matched 2 12
__OUT__

test_oE -e 42 'pattern matching after ;;&'
case 1 in
    0) echo not reached 0;;
    1) echo matched 1; (exit 12);;&
    2) echo not reached 2;;
    1) echo matched 2 $?; (exit 42);;&
    2) echo not reached 3;;
esac
__IN__
matched 1
matched 2 12
__OUT__

# Existing shells disagree on the behavior of this case.
test_oE 'exit status in case command with subject containing command substitution'
case $(echo 1; exit 42) in
    1) echo $?
esac
__IN__
0
__OUT__

# Many existing shells behave this way (with the notable exception of ksh).
test_OE -e 0 'exit status of case command with subject containing command substitution'
case $(echo 1; exit 42) in esac
__IN__

test_O -d -e 2 'in without case'
in
__IN__

test_O -d -e 2 ';; outside case (at beginning of line)'
;;
__IN__

test_O -d -e 2 ';; outside case (after simple command)'
echo foo;;
__IN__

test_O -d -e 2 'esac without case'
esac
__IN__

test_O -d -e 2 'case followed by EOF'
case
__IN__

test_O -d -e 2 'case followed by symbol'
case </dev/null
__IN__

test_O -d -e 2 'case followed by newline'
case
    1 in 1) echo not reached;; esac
__IN__

test_O -d -e 2 'word followed by EOF'
case 1
__IN__

test_O -d -e 2 'word followed by symbol'
case 1 </dev/null
__IN__

test_O -d -e 2 'in followed by EOF'
case 1 in
__IN__

test_O -d -e 2 'in followed by invalid symbol'
case 1 in </dev.null
__IN__

test_O -d -e 2 '( followed by EOF'
case 1 in (
__IN__

test_O -d -e 2 'invalid symbol in pattern'
case 1 in a</dev.null
__IN__

test_O -d -e 2 'missing pattern (separated by |)'
case 1 in |foo) esac
__IN__

test_O -d -e 2 'missing pattern (separated by parenthesis)'
case 1 in ) esac
__IN__

test_O -d -e 2 'separator followed by EOF'
case 1 in (1|
__IN__

test_O -d -e 2 'pattern followed by EOF'
case 1 in 1
__IN__

test_O -d -e 2 'pattern followed by esac (after one pattern)'
case 1 in 1 esac
__IN__

test_O -d -e 2 'pattern followed by esac (after two patterns)'
case 1 in 1|2 esac
__IN__

test_O -d -e 2 ') followed by EOF'
case 1 in 1)
__IN__

test_O -d -e 2 'inner command followed by EOF'
case 1 in 1) echo not reached
__IN__

test_O -d -e 2 'missing in-esac (in grouping)'
{ case 1 }
__IN__

test_O -d -e 2 'missing esac (in grouping)'
{ case 1 in *) }
__IN__
