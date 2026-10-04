# Castalia filesystem core and native CLI

`castalia-fs` stores immutable, content-addressed snapshots in an owner-private
local directory. It does not access the browser's OPFS workspace and does not
upload to Sia or publish a mutable/on-chain head.

The native CLI supports `import`, `export`, `verify`, `list`, and `read`. Its
`push` command copies one pinned snapshot to a second **local Castalia object
store** (which may be on a separately mounted disk), then reads every copied
object back before reporting success. The caller must give positive logical
object and byte limits. A dry run checks the source snapshot and limits
without opening the target:

```text
castalia-fs --store /private/source import /path/to/files --namespace <content-id>
castalia-fs --store /private/source push <snapshot-id> /private/target \
  --max-objects 10000 --max-bytes 134217728 --dry-run
castalia-fs --store /private/source push <snapshot-id> /private/target \
  --max-objects 10000 --max-bytes 134217728
castalia-fs --store /private/target verify <snapshot-id>
```

An interrupted push can be retried: records are immutable and published
atomically, and success is withheld until full readback. The command makes no
network-availability, encryption, remote durability, or Wallet-recovery claim.
For those guarantees, a separate authenticated transport and acceptance gate
are required.
