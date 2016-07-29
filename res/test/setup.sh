set -eo pipefail

main() {
    cd "$(dirname "$0")"
    local pakdir=$(pwd)
    local pakfile="$pakdir/pak0.pak"
    local paksum="85fc9cee2035b66290da1e33be2ac86b"

    if ! [[ -f "$pakfile" ]]; then
        wget "http://www.mirafiori.com/ftp/pub/gaming/pak0.pak"
    fi

    # verify MD5
    if command -v 'md5sum' >/dev/null; then
        # probably GNU
        local actual="$(md5sum "$pakfile" | cut -c -32)"
        if ! [[ "$paksum" == "$actual" ]]; then
            printf "Bad checksum on $pakfile (was %s, should be %s)" "$actual" "$paksum"
            exit 1
        fi

    elif command -v 'md5' >/dev/null; then
        # probably BSD / OS X
        local actual="$(md5 -q "$pakfile")"
        if ! [[ "$paksum" == "$actual" ]]; then
            printf "Bad checksum on $pakfile (was %s, should be %s)" "$actual" "$paksum"
            exit 1
        fi

    else
        # can't verify checksum
        echo "No MD5 utility found, exiting...";
        exit 1
    fi

    # echo $(cd "$(dirname "$0")"; pwd)
}

main
