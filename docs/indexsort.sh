# This shell script checks if the words listed in the index
# (src/topic_index.md) are sorted in dictionary order.
# To do so, this script:
#
# 1. Extracts lines between "# Index" and "## Symbols"
# 2. Excludes empty lines
# 3. Extracts words in square brackets for each line
# 4. Excludes non-alphanumeric characters from the extracted words
# 5. Checks if the extracted words are sorted in dictionary order

set -Ceu
unset CDPATH
cd -P -- "$(dirname "$0")"
export LC_ALL=C

index_file="src/topic_index.md"

awk '
    /^# Index$/ { in_index = 1; next }
    /^## Symbols$/ { in_index = 0; exit }
    in_index {
        # Extract text between first [ and ]
        start = index($0, "[")
        end = index($0, "]")
        if (start > 0 && end > start) {
            word = substr($0, start + 1, end - start - 1)
            # Remove backticks and other non-alphanumeric chars
            gsub(/[^a-zA-Z0-9 ]/, "", word)
            # Convert to lowercase
            print tolower(word)
        }
    }
' "$index_file" |
sort -c

# Also check if the "Symbols" section is sorted in ASCII order
awk '
    /^## Symbols$/ { in_symbols = 1; next }
    in_symbols {
        print $0
    }
' "$index_file" | grep . | sort -c
