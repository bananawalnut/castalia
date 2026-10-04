//! Local-only CLI. Push copies to an explicitly selected local object store;
//! there are no keys, remote uploads or automatic mirrors.
#[cfg(all(unix, not(target_arch = "wasm32")))]
mod cli {
    use castalia_filesystem_core::native::{DiskStore, export_directory, import_directory};
    use castalia_filesystem_core::*;
    use clap::{Parser, Subcommand};
    use std::io::Write;
    use std::path::PathBuf;

    #[derive(Parser)]
    #[command(about = "Local immutable directory snapshots (no remote publication)")]
    struct Args {
        #[arg(long)]
        store: PathBuf,
        #[command(subcommand)]
        command: Command,
    }
    #[derive(Subcommand)]
    enum Command {
        /// Mount a local pinned snapshot read-only (never fetches remotely).
        #[cfg(feature = "fuse")]
        Mount {
            snapshot: String,
            mountpoint: PathBuf,
            /// Validate metadata and mountpoint without changing OS mounts.
            #[arg(long)]
            check_only: bool,
        },
        Import {
            source: PathBuf,
            #[arg(long)]
            namespace: String,
        },
        /// Copy a pinned snapshot to another local store and verify every object there.
        Push {
            snapshot: String,
            target_store: PathBuf,
            /// Maximum logical object writes, including duplicates.
            #[arg(long)]
            max_objects: usize,
            /// Maximum total logical bytes transferred, including duplicates.
            #[arg(long)]
            max_bytes: u64,
            /// Verify the source and budget without opening the target store.
            #[arg(long)]
            dry_run: bool,
        },
        Export {
            snapshot: String,
            destination: PathBuf,
        },
        Verify {
            snapshot: String,
        },
        List {
            snapshot: String,
            #[arg(default_value = "/")]
            path: String,
        },
        Read {
            snapshot: String,
            path: String,
            #[arg(long, default_value_t = 0)]
            offset: u64,
            #[arg(long, default_value_t = MAX_READ_BYTES)]
            length: usize,
        },
    }
    fn id(value: String) -> Result<ContentId, Error> {
        ContentId::try_from(value)
    }
    struct VerifyOnly;
    impl ObjectWriter for VerifyOnly {
        async fn put(&self, bytes: &[u8]) -> Result<ContentId, Error> {
            Ok(ContentId::for_bytes(bytes))
        }
    }
    async fn verify_snapshot<R: ObjectReader>(
        reader: &R,
        snapshot: ContentId,
        budget: transfer::TransferBudget,
    ) -> Result<transfer::TransferReport, Error> {
        transfer::copy_snapshot(reader, &VerifyOnly, snapshot, budget).await
    }
    pub fn run() -> Result<(), Error> {
        let args = Args::parse();
        let store = DiskStore::open(&args.store)?;
        #[cfg(feature = "fuse")]
        if let Command::Mount {
            snapshot,
            mountpoint,
            check_only,
        } = &args.command
        {
            let map_error = |e| Error::Provider(format!("FUSE: {e:?}"));
            let fs =
                castalia_filesystem_core::fuse::LocalSnapshotFs::open(store, id(snapshot.clone())?)
                    .map_err(map_error)?;
            let _mountpoint = fs.check_mountpoint(mountpoint).map_err(map_error)?;
            if *check_only {
                println!(
                    "validated pinned root {}, {} nodes, {} indexed bytes; no mount performed",
                    String::from(fs.root()),
                    fs.node_count(),
                    fs.index_bytes()
                );
                return Ok(());
            }
            return castalia_filesystem_core::fuse::mount_local(fs, &_mountpoint);
        }
        futures::executor::block_on(async {
            match args.command {
                #[cfg(feature = "fuse")]
                Command::Mount { .. } => Err(Error::Invalid(
                    "mount must run outside the async local CLI executor",
                ))?,
                Command::Import { source, namespace } => {
                    let metadata = std::fs::symlink_metadata(&source)
                        .map_err(|e| Error::Transport(e.to_string()))?;
                    if !metadata.is_dir() || metadata.file_type().is_symlink() {
                        return Err(Error::Invalid("source must be a directory, not a symlink"));
                    }
                    let source = std::fs::canonicalize(&source)
                        .map_err(|e| Error::Transport(e.to_string()))?;
                    if store.path().starts_with(&source) {
                        return Err(Error::Invalid("store must be outside imported tree"));
                    }
                    let snapshot = import_directory(&store, &source, id(namespace)?).await?;
                    println!("{}", String::from(snapshot));
                }
                Command::Push {
                    snapshot,
                    target_store,
                    max_objects,
                    max_bytes,
                    dry_run,
                } => {
                    if max_objects == 0 || max_bytes == 0 {
                        return Err(Error::Invalid("push budget must be positive"));
                    }
                    let snapshot = id(snapshot)?;
                    let budget = transfer::TransferBudget {
                        max_objects,
                        max_bytes,
                    };
                    // Full preflight catches missing/corrupt source objects and
                    // budget exhaustion before the destination is touched.
                    let expected = verify_snapshot(&store, snapshot, budget).await?;
                    if dry_run {
                        println!(
                            "dry run: verified {} objects, {} bytes for root {}; no copy performed",
                            expected.objects,
                            expected.bytes,
                            String::from(snapshot)
                        );
                    } else {
                        let target = DiskStore::open(&target_store)?;
                        if target.path() == store.path() {
                            return Err(Error::Invalid(
                                "push target must differ from source store",
                            ));
                        }
                        let copied =
                            transfer::copy_snapshot(&store, &target, snapshot, budget).await?;
                        if copied != expected {
                            return Err(Error::Integrity);
                        }
                        // Success means a full independent read through the
                        // destination adapter, not merely successful writes.
                        let readback = verify_snapshot(&target, snapshot, budget).await?;
                        if readback != expected {
                            return Err(Error::Integrity);
                        }
                        println!(
                            "verified local push of root {}: {} objects, {} bytes",
                            String::from(snapshot),
                            readback.objects,
                            readback.bytes
                        );
                    }
                }
                Command::Export {
                    snapshot,
                    destination,
                } => export_directory(&store, id(snapshot)?, &destination).await?,
                Command::Verify { snapshot } => {
                    let view = SnapshotView::open(&store, id(snapshot)?).await?;
                    let nodes = view.validate_tree().await?;
                    let report = verify_snapshot(
                        &store,
                        view.id(),
                        transfer::TransferBudget {
                            max_objects: usize::MAX,
                            max_bytes: u64::MAX,
                        },
                    )
                    .await?;
                    println!(
                        "verified {nodes} nodes, {} objects, {} bytes",
                        report.objects, report.bytes
                    );
                }
                Command::List { snapshot, path } => {
                    let view = SnapshotView::open(&store, id(snapshot)?).await?;
                    for entry in view.list(&path).await? {
                        println!(
                            "{:?}\t{}\t{}",
                            entry.node.kind, entry.node.inode, entry.name
                        );
                    }
                }
                Command::Read {
                    snapshot,
                    path,
                    offset,
                    length,
                } => {
                    let view = SnapshotView::open(&store, id(snapshot)?).await?;
                    let bytes = view.read_range(&path, offset, length).await?;
                    std::io::stdout()
                        .lock()
                        .write_all(&bytes)
                        .map_err(|e| Error::Transport(e.to_string()))?;
                }
            }
            Ok(())
        })
    }
}
#[cfg(all(unix, not(target_arch = "wasm32")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    cli::run().map_err(Into::into)
}
#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
fn main() {
    eprintln!("castalia-fs CLI requires native Unix; use the portable library on WASM.");
}
