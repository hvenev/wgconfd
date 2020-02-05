wgconfd(8)

# NAME

wgconfd - configuration manager for WireGuard

# SYNOPSIS

*wgconfd* _IFNAME_ _CONFIG_++
*wgconfd* --cmdline _IFNAME_ _ARGS_...++
*wgconfd* --check-source _PATH_

# DESCRIPTION

In the 1st and 2nd form, run wgconfd on the network interface named IFNAME. In
the 1st form, CONFIG is the path to the configuration file, the format of which
is described in *wgconfd*(5). In the 2nd form, the configuration is provided on
the command line. The way this is done is described in the _COMMAND-LINE
CONFIGURATION_ section.

In the 3rd form, wgconfd checks the validity of a source JSON. PATH is a path
to a local file.

# COMMAND-LINE CONFIGURATION

While the usual way to configure wgconfd is through a file, in some cases it
is easier to pass everything on the command line. The arguments consist of a
sequence of global options, sources, and peers:

	min_keepalive _SEC_++
max_keepalive _SEC_++
refresh_sec _SEC_++
source _NAME_ _URL_ [psk _PATH_] [ipv4 _NET_,_NET_,...] [ipv6 _NET_,_NET_,...] [required] [allow_road_warriors | deny_road_warriors]++
peer _PUBKEY_ [endpoint _IP_:_PORT_] [psk _PATH_] [keepalive _SEC_] [source _NAME_]

# EXIT VALUE

When managing an interface, wgconfd does not exit. When checking the validity of
a source, a nonzero exit status would indicate a problem with the source.

# ENVIRONMENT

*wgconfd* uses the following environment variables:

	WG - the *wg*(8) command. Used for configuring the interface. Default: _wg_

	CURL - the *curl*(1) command. Used for fetching sources. Default: _curl_

	CACHE_DIRECTORY - the path where sources are cached.

	RUNTIME_DIRECTORY - the path where the runtime state is preserved.

# SEE ALSO

*wgconfd*(5), *wg*(8), *curl*(1)
