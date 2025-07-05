# This script runs documentation tests for markdown files in the docs directory.
# It extracts code examples from the markdown files and executes them to ensure
# that all documented code snippets are correct and up-to-date.
#
# Usage:
#   ./doctest.sh [<file>...]
#
# Input parameters:
#   Without any operands, the script scans all markdown files in the directory
#   where the script is located and runs the tests on each file.
#   If one or more file names are provided, the script runs the tests only on
#   those files.
#
# Expected output:
#   The script prints the results of failed tests.
#
# The exit status of the script indicates whether all tests passed.
#
# This script examines attributes attached to code blocks to determine how to
# handle them. The following attributes are recognized:
#
# - `shell`: The code block is a shell session. Lines starting with `$ ` and `>
#   ` are commands to be executed, and the rest is expected output.
# - `sh`: The code block is a shell script. The entire block is parsed as a
#   shell script, and its syntax is checked.
# - `ignore`: The code block is ignored and not tested.
# - `no_run`: The code block is not executed, but its syntax is checked.
# - `one_shot`: The code block is executed, but the output is checked only once
#   against the expected output. (By default, the output is checked for each
#   input/output pair.)
# - `hidelines=<prefix>`: The prefix is stripped from each line of the code
#   block before processing. See also the mdBook documentation about this
#   attribute: <https://rust-lang.github.io/mdBook/format/configuration/renderers.html?highlight=hidelines#outputhtmlcode>
#
# Script structure:
#
# 1. Parses command-line arguments and determines which markdown files to
#    process.
# 2. For each file, scans for code blocks and interprets their attributes.
# 3. For shell session and script blocks, extracts commands and expected output.
# 4. Executes or checks the extracted code as appropriate, comparing actual
#    output to expected.
# 5. Reports any mismatches or syntax errors, and sets the exit status
#    accordingly.

set -Ceu

while getopts '' option; do
  case $option in
    (\?)
      printf '%s: invalid option: -%s\n' "$0" "$OPTARG" >&2
      exit 2
      ;;
  esac
done
shift $((OPTIND - 1))

if ! [ "${YASH+set}" ]; then
    YASH="$(cargo run --quiet -- -c 'printf "%s\n" "$0"')"
    case "$YASH" in
    (/*) ;;
    (*)  YASH="${PWD%/}/$YASH" ;;
    esac
    export YASH
fi

if [ $# -eq 0 ]; then
    # scan all files

    # prepare for parallel execution
    nproc="$({ nproc || sysctl -n hw.logicalcpu; } 2>/dev/null || echo 1)"
    # if xargs supports -P, use it
    if [ "$nproc" -gt 1 ] && echo true | xargs -0 -L 1 -P "$nproc" sh -c 2>/dev/null; then
        find "$(dirname -- "$0")" -type f -name '*.md' -print0 | xargs -0 -L 1 -P "$nproc" "$0"
    else
        exec find "$(dirname -- "$0")" -type f -name '*.md' -exec "$0" {} +
    fi
    exit
fi

tmpdir="${TMPDIR:-/tmp}"
tmpdir="${tmpdir%/}/doctest-$$"
trap 'rm -rf -- "$tmpdir"' EXIT
trap 'rm -rf -- "$tmpdir"; exit 99' HUP INT QUIT TERM
mkdir -p -- "$tmpdir"

script="$tmpdir/script.sh"
expected="$tmpdir/expected.log"
actual="$tmpdir/actual.log"

success="true"

nextline() {
    lineno=$((lineno + 1))
    IFS= read -r line && line="${line#"$indent"}"
}
gethidelines() {
    hidelines=
    for attr do
        case "$attr" in
        (hidelines=*)
            hidelines="${attr#hidelines=}"
            ;;
        esac
    done
}
applyhidelines() {
    line=${line#"$hidelines"}
}
checksyntax() {
    case " $* " in
    (*' ignore '*)
        ;;
    (*)
        if ! "$YASH" -n < "$script"; then
            printf '%s:%d: error: syntax error in script\n' "$file" "$blocklineno" >&2
            success="false"
        fi
        ;;
    esac
}
checkoutput() {
    case " $* " in
    (*' ignore '*)
        ;;
    (*' no_run '*)
        checksyntax "$@"
        ;;
    (*)
        if
            (cd -- "$tmpdir" && "$YASH" < "$script" >| "$actual" 2>&1)
            ! diff -u "$expected" "$actual"
        then
            printf '%s:%d: error: script output does not match\n' "$file" "$blocklineno" >&2
            success="false"
        fi
        ;;
    esac
}
checkoutputsofar() {
    case " $* " in
    (*' one_shot '*)
        ;;
    (*)
        checkoutput "$@"
        ;;
    esac
}

for file do
    exec < "$file"
    lineno=0

    while indent=''; nextline; do
        indent="${line%%[![:space:]]*}"
        line="${line#"$indent"}"

        case "$line" in
        ('```'*)
            blocklineno="$lineno"

            # split attributes
            attrs="${line#'```'}"
            oldifs="$IFS"
            IFS=', 	'
            set -- $attrs
            IFS="$oldifs"

            gethidelines "$@"

            case "${1-}" in
            (sh)
                # copy script to temporary file
                while nextline && test "$line" != '```'; do
                    applyhidelines
                    printf '%s\n' "$line"
                done >| "$script"

                # check script syntax
                checksyntax "$@"
                ;;
            (shell)
                # clear file content
                >| "$script" >| "$expected"

                # read script and expected output from the block
                # run the script and check the output against the expected
                while nextline; do
                    if [ "$line" = '```' ]; then
                        checkoutput "$@"
                        break
                    fi

                    applyhidelines
                    case "$line" in
                    ('$ '*)
                        checkoutputsofar "$@"
                        printf '%s\n' "${line#'$ '}" >> "$script"
                        ;;
                    ('> '*)
                        printf '%s\n' "${line#'> '}" >> "$script"
                        ;;
                    (*)
                        printf '%s\n' "$line" >> "$expected"
                        ;;
                    esac
                done
                ;;
            (*)
                # skip without any check
                while nextline && test "$line" != '```'; do :; done
                ;;
            esac
            ;;
        esac
    done
done

"$success"
