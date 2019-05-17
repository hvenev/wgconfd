v0.1.2
---

 - The interface is now stored under `$RUNTIME_DIRECTORY` if it is set. The
state file should be removed when the interfaces is reset.

 - The daemon's config can now be provided via the command line. TOML config
support has been made optional and is enabled by default.

 - There is now an init script for OpenWRT procd that gets the configuration
from UCI and passes it via command line.
