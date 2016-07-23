set -eo pipefail

main() {
    local pakfile="$PWD/pak0.pak"
    local paksum="85fc9cee2035b66290da1e33be2ac86b"

    if ! [[ -f "$pakfile" ]]; then
        wget "http://www.mirafiori.com/ftp/pub/gaming/pak0.pak"
    fi

    # verify MD5
    if command -v 'md5sum' >/dev/null; then
        # probably GNU environment
        if ! [[ "$paksum" == $(md5sum "$pakfile")[0] ]]; then
            echo "Bad checksum on $pakfile."
            exit
        fi

    elif command -v 'md5' >/dev/null; then
        # probably OS X
        if ! [[ "$paksum" == $(md5 -q "$pakfile") ]]; then
            echo "Bad checksum on $pakfile."
            exit
        fi

    else
        # can't verify checksum
        echo "No MD5 utility found, exiting...";
        exit
    fi

    # echo $(cd "$(dirname "$0")"; pwd)
}

main
