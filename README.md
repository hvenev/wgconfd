wgconfd
===

wgconfd is a configuration manager for [WireGuard](https://wireguard.com).

General behavior
---
`wgconfd INTERFACE CONFIG` starts a process that manages some peers of a WireGuard interface. It adds/overwrites peers it knows about and removes peers once they disappear from its view. It leaves any peers it has never seen intact.

Configuration
---

The configuration consists of a set of sources. A source consists of a URL (required), a set of allowed IP address ranges (optional, defaults to nothing), and a preshared key (optional):

```toml
refresh_sec = 1200 # default
min_keepalive = 10 # default
max_keepalive = 0 # default, means "never"

[source.remote1]
url = "https://wg.example.org/peers.json"
ipv4 = [ "172.16.0.0/12", "192.168.5.0/24" ]
ipv6 = [ "2001:db8::/32" ]

[source.remote2]
url = "https://wg.example.com/peers.json"
ipv4 = [ "172.16.0.0/12", "192.168.6.0/24" ]
ipv6 = [ "2001:db8:1234:/48" ]
psk = "GBRwvlGYEcHqe+ft+px5I9dMAGWAqsghftSDz2PhoM8="

[source.local-user1]
url = "file:///home/user1/.config/wg-dev.json"
ipv4 = [ "172.16.5.54/32" ]

[source.local-user2]
url = "file:///home/user2/.config/wg-dev.json"
ipv6 = [ "2001:db8::5/128" ]
```

All IP address ranges from the source URL not entirely contained within the config are discarded - if a source claims `0.0.0.0/0` but the config only allows `10.0.0.0/8`, nothing is allowed.

The preshared key is applied to all peers defined in a source. If a single peer is defined in multiple sources, both the endpoint and preshared key are taken from a single source chosen nondeterministically, but all IP ranges are allowed.

It is possible to override the preshared key for a specific public key, and to restrict the source that can define that peer:

```toml
[peer."yIOdSFrFQ1WPYS6IUWCsRjzw2Iqq0HMcyVVEXu5z+nM="]
source = "remote2"
psk = "QJmzt2PpKx8g98qrOtsNR4tB1bik+fMSabNNXCC5OUU="
```

Alternative configuration
---

There is an alternative configuration mechanism intended for integration with other software: `wgconfd --cmdline INTERFACE ARGS...`

The arguments are a sequence of global options and sources:

 - `min_keepalive TIME`
 - `max_keepalive TIME`
 - `refresh_sec TIME`
 - `source NAME URL [psk PSK] [ipv4 NET,NET,...] [ipv6 NET,NET,...] [required]`
 - `peer PUBKEY [psk PSK] [source NAME]`

Source format
---

The source describes a list of peers with their associated `endpoint` address (required), `keepalive` (optional, defaults to never), and `ipv4` and `ipv6` ranges (optional, defaults to nothing):

```json
{
	"servers": [{
		"public_key": "hw0U7vI2rhjG9mQ34CUKO6M4dIF9e8ofKj5N6cAPtwY=",
		"endpoint": "198.51.100.66:656",
		"ipv4": [ "10.1.2.0/24" ]
	}, {
		"public_key": "nlFVtJrOwR2sVJji6NQjXnv//GVUK5W9T7ftkSnYPA8=",
		"endpoint": "[2002:cb00:71af::4]:656",
		"ipv4": [ "10.1.3.0/24" ]
	}],
}
```

### Road warriors
wgconfd also supports roaming peers called "road warriors":

```json
{
	...
	"road_warriors": [{
		"public_key": "YJ0Ye/Z/f+kzMu5au8JL/OP+cMs0eRsJPSQ9FZIa7Sk=",
		"base": "ymTvQHxgEDacZq90T/1dYR4ARvtbBTH4rIHab83WFBY=",
		"ipv4": [ "10.2.5.44/32" ]
	}, ...]
}
```

A road warrior does not have an endpoint and does not run wgconfd - it is instead expected to only talk to its base server peer.

On the base peer, a WireGuard peer is created for the road warrior. On all other peers an additional allowed IP address is added for the base.

A road warrior from one source can use a server from another source, but allowed IPs are always checked against the source that contains the road warrior definition.

### Configuration updates
The root object can contain a field `"next"` with an `"update_at"` timestamp and another configuration:

```json
{
	"servers": [{
		"public_key": "hw0U7vI2rhjG9mQ34CUKO6M4dIF9e8ofKj5N6cAPtwY=",
		"endpoint": "198.51.100.66:656",
		"ipv4": [ "10.1.2.0/24" ]
	}, {
		"public_key": "nlFVtJrOwR2sVJji6NQjXnv//GVUK5W9T7ftkSnYPA8=",
		"endpoint": "[2002:cb00:71af::4]:656",
		"ipv4": [ "10.1.3.0/24" ]
	}],
	"road_warriors": [{
		"public_key": "YJ0Ye/Z/f+kzMu5au8JL/OP+cMs0eRsJPSQ9FZIa7Sk=",
		"base": "ymTvQHxgEDacZq90T/1dYR4ARvtbBTH4rIHab83WFBY=",
		"ipv4": [ "10.2.5.44/32" ]
	}],
	"next": {
		"update_at": "2033-05-18T03:33:20Z",
		"servers": [{
			"public_key": "hw0U7vI2rhjG9mQ34CUKO6M4dIF9e8ofKj5N6cAPtwY=",
			"endpoint": "198.51.100.66:656",
			"ipv4": [ "10.1.2.0/24" ]
		}, {
			"public_key": "nlFVtJrOwR2sVJji6NQjXnv//GVUK5W9T7ftkSnYPA8=",
			"endpoint": "[2002:cb00:71af::4]:656",
			"ipv4": [ "10.1.3.0/25" ]
		}, {
			"public_key": "nlFVtJrOwR2sVJji6NQjXnv//GVUK5W9T7ftkSnYPA8=",
			"endpoint": "[2001:db8:ddcc:bbaa::5]:565",
			"ipv4": [ "10.1.3.128/25" ]
		}],
		"road_warriors": [{
			"public_key": "YJ0Ye/Z/f+kzMu5au8JL/OP+cMs0eRsJPSQ9FZIa7Sk=",
			"base": "nlFVtJrOwR2sVJji6NQjXnv//GVUK5W9T7ftkSnYPA8=",
			"ipv4": [ "10.2.5.44/32" ]
		}]
	}
}
```

All instances of `wgconfd` using that source will switch to the new configuration at the same time according to their system clocks. Note that the regular mechanism for updates still applies - to cancel an update, remove the `"next"` field early enough so that all machines refresh the source before `"update_at"`.
