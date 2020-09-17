wgconfd
===

wgconfd is a configuration manager for [WireGuard](https://wireguard.com/).

Overview
---
`wgconfd INTERFACE CONFIG` starts a process that manages some peers of a WireGuard interface. It adds/overwrites peers it knows about and removes peers once they disappear from its view. It leaves any peers it has never seen intact.

Peers are defined in sources. A source is a JSON file served over a protocol that `curl` understands.

A "server" peer has a known IP address and talks to other servers directly. A "road warrior" peer communicates with everyone through its base server.

Configuration
---

The main configuration file is written in [toml](https://github.com/toml-lang/toml).

```toml
refresh_sec = 1200 # default
min_keepalive = 10 # default
max_keepalive = 0 # default, means "never"

[[source]]
name = "remote1"
url = "https://wg.example.org/peers.json"
ipv4 = [ "172.16.0.0/12", "192.168.5.0/24" ]
ipv6 = [ "2001:db8::/32" ]

[[source]]
name = "remote2"
url = "https://wg.example.com/peers.json"
ipv4 = [ "172.16.0.0/12", "192.168.6.0/24" ]
ipv6 = [ "2001:db8:1234:/48" ]
psk = "/path/to/psk/file"
allow_road_warriors = false

[[source]]
name = "local-user1"
url = "file:///etc/wireguard/example/user1.json"
ipv4 = [ "172.16.5.54/32" ]

[[source]]
name = "local-user2"
url = "file:///etc/wireguard/example/user2.json"
ipv6 = [ "2001:db8::5/128" ]
```

All IP address ranges from the source URL not entirely contained within the ones configured are discarded - if a source claims `0.0.0.0/0` but the config only allows `10.0.0.0/8`, nothing is allowed.

The preshared key is applied to all peers defined in a source. If a single peer is defined in multiple sources, both the endpoint and preshared key are taken from the first source that defines it.

It is possible to override some options for a specific public key, and/or to restrict the source that can define that peer:

```toml
[peer."yIOdSFrFQ1WPYS6IUWCsRjzw2Iqq0HMcyVVEXu5z+nM="]
source = "remote2"
endpoint = "[2001:db8::6]:10656"
psk = "/path/to/psk/file"
keepalive = 20
```

### Alternative configuration

There is an alternative configuration mechanism intended for integration with other software: `wgconfd --cmdline INTERFACE ARGS...`

The arguments are a sequence of global options and sources:

 - `min_keepalive SEC`
 - `max_keepalive SEC`
 - `refresh_sec SEC`
 - `source NAME URL [psk PATH] [ipv4 NET,NET,...] [ipv6 NET,NET,...] [required] [allow_road_warriors | deny_road_warriors]`
 - `peer PUBKEY [endpoint IP:PORT] [psk PATH] [keepalive SEC] [source NAME]`

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
		"base": "hw0U7vI2rhjG9mQ34CUKO6M4dIF9e8ofKj5N6cAPtwY=",
		"ipv4": [ "10.2.5.44/32" ]
	}, ...]
}
```

A road warrior does not typically run wgconfd. It is instead expected to only talk to its base server peer.

On the base peer, a WireGuard peer is created for the road warrior. On all other peers the allowed IP address ranges of the road warrior are added to its base instead.

A road warrior from one source can use a server from another source, but allowed IPs are always checked against the source that contains the road warrior definition.

The `allow_road_warriors` option in `[[source]]` sections can be used to deny being the base of road warriors from certain sources.

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
		"base": "hw0U7vI2rhjG9mQ34CUKO6M4dIF9e8ofKj5N6cAPtwY=",
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
			"public_key": "JjSETJ9ACv0sTTEtBE2qp9q4vbeq1i5suwWaJCuncFo=",
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

All instances of `wgconfd` using that source will switch to the new configuration at the specified time according to their system clocks. Note that the regular mechanism for updates still applies - to cancel an update, remove the `"next"` field early enough so that all machines refresh the source before `"update_at"`.

Operating system support
---

wgconfd should work on all operating systems that provide the `wg` and `curl` commands.

### systemd-based Linux distributions

Sample unit files are provided in the `dist/systemd` directory:

 - `wgconfd@IFNAME.service` runs wgconfd on the network interface `IFNAME` using configuration in `/etc/wireguard/IFNAME.toml`. The service expects that the interface has already been created and the prviate key has been set.
 - `wgconfd-state@IFNAME.service` should be restarted every time the network interface loses its configuration, for example when wg-quick is restarted.

A Fedora source package is available at [https://git.venev.name/hristo/fedora/rust-wgconfd/].

### OpenWRT

There is an OpenWRT netifd protocol script in `dist/netifd`. The global options are set in the interface section in `/etc/config/network`. Sources and peers are defined in `wgconfd_source_IFNAME`/`wgconfd_peer_IFNAME` sections in the same file:

```sh
config interface 'wg0'
	option proto 'wgconfd'
	option listen_port '656'
	option private_key 'uAoL9qoAFbAPg46NxIQJ36Zc5gJaYDBleL2iGEa8SEA='
	list ip6addr '2002:db8:1:1/48'
	list ipaddr '10.4.0.1/10'

config wgconfd_source_wg0
	option name 'source1'
	option url 'https://wg.example.org/peers.json'
	list ipv4 '10.5.0.0/16'
	list ip6addr '2002:db8:2:3/48'

config wgconfd_source_wg0
	option name 'source2'
	option url 'https://wg.example.com/peers.json'
	list ipv4 '10.6.0.0/16'

config wgconfd_peer_wg0
	option public_key 'dJyitquxsM3gf8a8yVDko6Se0sKrXi+glUTQN4mPZCo='
	option source 'source2'
	option psk '/etc/wgconfd-psk/example.com-machine1'
```
