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
#   If any code snippet fails, the script exits with a non-zero status.
#
# If all tests pass, the script exits with a zero status.

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

if [ $# -eq 0 ]; then
    # scan all files
    exec find "$(dirname -- "$0")" -type f -name '*.md' -exec "$0" {} +
    exit
fi

if ! [ "${YASH+set}" ]; then
    YASH="$(cargo run --quiet -- -c 'printf "%s\n" "$0"')"
    export YASH
fi

tmpdir="${TMPDIR:-/tmp}"
tmpdir="${tmpdir%/}/tmp.$$"
trap 'rm -rf -- "$tmpdir"' EXIT
trap 'rm -rf -- "$tmpdir"; exit 99' HUP INT QUIT TERM
mkdir -p "$tmpdir"

script="$tmpdir/script.sh"
expected="$tmpdir/expected.log"
actual="$tmpdir/actual.log"

success="true"

nextline() {
    lineno=$((lineno + 1))
    IFS= read -r line
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
        if "$YASH" < "$script" >| "$actual" 2>&1; ! diff -u "$expected" "$actual"; then
            printf '%s:%d: error: script output does not match\n' "$file" "$blocklineno" >&2
            success="false"
        fi
        ;;
    esac
}

for file do
    exec < "$file"
    lineno=0

    while nextline; do
        # TODO support indented code blocks
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
                        checkoutput "$@"
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
