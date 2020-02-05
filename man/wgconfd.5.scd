wgconfd(5)

# NAME

wgconfd - configuration file

# GLOBAL OPTIONS

The following global options are available:

*min_keepalive*
	The minimum value for the persistent keepalive timeout, in seconds. Any peer
	with a smaller timeout uses this one instead. Set to 0 to disable the
	timeout altogether.

	Default: _10_

*max_keepalive*
	The maximum value for the persistent keepalive timeout, in seconds. Any peer
	with a larger (or missing) timeout uses this one instead. If set to 0, there
	is no maximum.

	Default: _0_

*refresh_sec*
	The time between configuration updates, in seconds.

	Default: _1200_

*cache_directory*
	Path to the cache directory.

	Default: _$CACHE_DIRECTORY_

*runtime_directory*
	Path to the runtime state directory.

	Default: _$RUNTIME_DIRECTORY_

# SOURCE SECTIONS

Sources are defined in *[[source]]* sections. The following options are available:

*name*
	The name used to identify the source in logs and in the cache. All sources
	should have distinct names. Should only contain characters that can be put
	in a filename. Required.

*url*
	The URL of the source. It must point to a JSON file following the format
	described in the README.

*ipv4*
	A list of allowed IPv4 networks, each of the form _"ADDR/LEN"_. All of the
	address bits after the prefix must be set to 0.

	If a source tries to assign a range of addresses to a peer and that range
	has addresses that are not listed in the *ipv4* configuration option, the
	entire range is discarded.

	Default: _[]_

*ipv6*
	A list of allowed IPv6 networks, each of the form _"ADDR/LEN"_. All of the
	address bits after the prefix must be set to 0.

	If a source tries to assign a range of addresses to a peer and that range
	has addresses that are not listed in the *ipv6* configuration option, the
	entire range is discarded.

	Default: _[]_

*psk*
	Path to a file containing the default preshared key used for all peers
	defined by this source.

	Default: no preshared key

*required*
	Boolean. If set to true, *wgconfd*(8) will fail to start if fetching the
	source fails.

	Default: _false_

*allow_road_warriors*
	Boolean. If set to false, road warriors from this source will not be allowed
	to use this interface machine as their base peer.

	Default: _true_

# PEER SECTIONS

In some cases one may want to override some settings for individual peers.
This can be achieved through *[[peer]]* sections:

*public_key*
	The public key of the peer for which the overrides apply, as a base64
	encoded string.

	Required.

*source*
	If specified, ignore attempts by other sources to define this peer. Note
	that even if this is set, other sources can add allowed IP addresses for
	the peer by creating road warriors.

	Default: do not restrict source

*endpoint*
	Override the endpoint address of the peer.

	Default: use the endpoint address from the source

*psk*
	Path to a preshared key to use for this peer.

	Default: the PSK of the source, if any

*keepalive*
	Override the persistent keepalive timeout for this peer. The value here is
	not affected by the *min_keepalive* and *max_keepalive* configuration
	options.

	Default: the keepalive value from the source, or infinite if not set,
	restricted by *min_keepalive* and *max_keepalive*

Note that having a *[[peer]]* section is not enough to create a peer - it must
also exist in one of the sources.
