v0.2.0
---

- Peer overrides can be specified in the main configuration file. An override
for a specific public key can contain a preshared key and can restrict the
source that can define the peer.

- The systemd service has been split in two. Restarting `wgconfd-state@.service`
also wipes the state. This service should be marked as `PartOf=` the service
that manages the interface. `wgconfd@.service` itself is
`PartOf=wgconfd-state@.service`.


v0.1.2
---

 - The interface is now stored under `$RUNTIME_DIRECTORY` if it is set. The
state file should be removed when the interfaces is reset.

 - The daemon's config can now be provided via the command line. TOML config
support has been made optional and is enabled by default.

 - There is now an init script for OpenWRT procd that gets the configuration
from UCI and passes it via command line.
