#!/bin/sh

WG=/usr/bin/wg
if [ ! -x "$WG" ]; then
        logger -t "wgconfd" "error: missing wgconfd (${WG})"
        exit 1
fi

CURL=/usr/bin/curl
if [ ! -x "$CURL" ]; then
        logger -t "wgconfd" "error: missing curl (${CURL})"
        exit 1
fi

WGCONFD=/usr/bin/wgconfd
if [ ! -x "$WGCONFD" ]; then
        logger -t "wgconfd" "error: missing wgconfd (${WGCONFD})"
        exit 1
fi

[ -n "$INCLUDE_ONLY" ] || {
	. /lib/functions.sh
	. ../netifd-proto.sh
	init_proto "$@"
}

proto_wgconfd_init_config() {
	proto_config_add_array 'ipaddr:ipaddr'
	proto_config_add_array 'ip6addr:ip6addr'
	proto_config_add_int 'mtu'

	proto_config_add_string 'private_key'
	proto_config_add_int 'listen_port'
	proto_config_add_string 'fwmark'

	proto_config_add_int 'refresh_sec'
	proto_config_add_int 'min_keepalive'
	proto_config_add_int 'max_keepalive'

	available=1
}

proto_wgconfd_setup__print() {
	local i
	for i; do
		# TODO: escape
		echo -n "$i "
	done
}

proto_wgconfd_setup__source() {
	local name val

	config_get name "$1" name
	[ -z "$name" ] && return
	config_get val "$1" url
	[ -z "$val" ] && return
	proto_wgconfd_setup__print source "$name" "$val"

	config_get val "$1" psk
	[ -n "$val" ] && proto_wgconfd_setup__print psk "$val"

	config_get_bool val "$1" required 0
	[ "$val" -eq 1 ] && proto_wgconfd_setup__print required

	config_get_bool val "$1" allow_road_warriors 1
	[ "$val" -eq 0 ] && proto_wgconfd_setup__print deny_road_warriors

	config_list_foreach "$1" ipv4 proto_wgconfd_setup__source_route ipv4 32

	config_list_foreach "$1" ipv6 proto_wgconfd_setup__source_route ipv6 128
}

proto_wgconfd_setup__source_route() {
	local p="$2"
	local maxlen="$3"
	local route=1
	set -- $1
	local r="$1"
	shift 1
	local i
	for i; do case "$i" in
		no-route)
			route=0
			;;
		*)
			true
			;;
	esac; done
	proto_wgconfd_setup__print "$p" "$r"
	if [ "$route" -eq 1 ]; then
		case "$r" in
			'')
				true
				;;
			*/*)
				echo "${p}_route ${r%/*} ${r##*/}" >> "$dir/update"
				;;
			*)
				echo "${p}_route $r $maxlen" >> "$dir/update"
				;;
		esac
	fi
}

proto_wgconfd_setup__peer() {
	local val

	config_get val "$1" key
	[ -z "$val" ] && return
	proto_wgconfd_setup__print public_key "$val"

	config_get val "$1" endpoint
	[ -n "$val" ] && proto_wgconfd_setup__print endpoint "$val"

	config_get val "$1" psk
	[ -n "$val" ] && proto_wgconfd_setup__print psk "$val"

	config_get val "$1" keepalive
	[ -n "$val" ] && proto_wgconfd_setup__print keepalive "$val"

	config_get val "$1" source
	[ -n "$val" ] && proto_wgconfd_setup__print source "$val"
}

proto_wgconfd__echo_addr() {
	case "$1" in
		'')
			true
			;;
		*/*)
			echo "${3}_address ${1%/*} ${1##*/}" >> "$dir/update"
			;;
		*)
			echo "${3}_address $1 $4" >> "$dir/update"
			;;
	esac
}

proto_wgconfd_setup() {
	local interface="$1" ifname="$2" i r
	if [ -z "$ifname" ]; then
		ifname="$interface"
	fi

	local mtu
	local private_key listen_port fwmark
	local refresh_sec min_keepalive max_keepalive
	json_get_vars mtu private_key listen_port fwmark refresh_sec min_keepalive max_keepalive

	if [ -z "$private_key" ]; then
		proto_notify_error "$interface" NO_PRIVATE_KEY
		proto_block_restart "$interface"
		exit
	fi

	[ -n "$fwmark" ] && fwmark="fwmark $fwmark"

	dir="/tmp/wgconfd/$interface"
	if [ -d "$dir" ]; then
		rm -rf "$dir"
	fi
	mkdir -p /tmp/wgconfd
	if ! mkdir -m 0700 "$dir" || ! mkdir "$dir/cache" || ! echo "$private_key" > "$dir/private" || ! true > "$dir/update" ; then
		proto_notify_error "$interface" FS_ERROR
		return 1
	fi

	json_for_each_item proto_wgconfd__echo_addr ipaddr ipv4 32
	json_for_each_item proto_wgconfd__echo_addr ip6addr ipv6 128

	wgconfd_command="$(
		proto_wgconfd_setup__print "$WGCONFD" --cmdline "$ifname"
		config_load network
		config_foreach proto_wgconfd_setup__source wgconfd_source_"$interface"
		config_foreach proto_wgconfd_setup__peer wgconfd_peer_"$interface"
	)"

	ip link del dev "$ifname" 2>/dev/null
	if ! ip link add dev "$ifname" mtu "${mtu:-1420}" type wireguard; then
		proto_notify_error "$interface" IFACE_ERROR
		exit
	fi

	"$WG" set "$ifname" private-key "$dir/private" listen-port "${listen_port:-656}" $fwmark
	r="$?"
	rm -f "$dir/private"
	if [ "$r" != 0 ]; then
		ip link del dev "$ifname" 2>/dev/null
		proto_notify_error "$interface" WG_ERROR
		exit
	fi

	proto_init_update "$ifname" 1 0
	proto_set_keep 0
	while read i r; do
		proto_add_"$i" $r
	done < "$dir/update"
	# rm -f "$dir/update"
	proto_send_update "$interface"


	proto_export "WG=$WG"
	proto_export "CURL=$CURL"
	proto_export "RUNTIME_DIRECTORY=$dir"
	proto_export "CACHE_DIRECTORY=$dir/cache"
	proto_run_command "$interface" $wgconfd_command
}

proto_wgconfd_teardown() {
	local interface="$1" ifname="$2" i r
	if [ -z "$ifname" ]; then
		ifname="$interface"
	fi

	proto_kill_command "$interface"
	ip link del dev "$ifname" 2>/dev/null
}

[ -n "$INCLUDE_ONLY" ] || {
        add_protocol wgconfd
}
