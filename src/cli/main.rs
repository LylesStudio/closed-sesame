//! ClosedSesame CLI — `sesame` binary.
//!
//! Commands: create, verify, list, mount, defrag

rust_i18n::i18n!("locales", fallback = "en");

macro_rules! ts {
    ($($args:tt)*) => {{
        rust_i18n::t!($($args)*).into_owned()
    }};
}

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use closed_sesame::app::{
    credentials::{Credential, prepare_credentials},
    fmt_size, parse_size,
    symlink::SymlinkRegistry,
    vault::{self, AddDisposition},
};
use closed_sesame::container::defrag;
use closed_sesame::error::Result;
use closed_sesame::i18n::{self, Locale, keys};
use std::path::PathBuf;
use std::process::ExitCode;
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "sesame", version)]
struct Cli {
    /// Interface language (en, zh-CN, ja)
    #[arg(long, global = true, value_parser = parse_lang)]
    lang: Option<Locale>,
    #[command(subcommand)]
    command: Commands,
}

fn parse_lang(value: &str) -> std::result::Result<Locale, String> {
    Locale::from_code(value).ok_or_else(|| format!("unsupported language: {value}"))
}

#[derive(Subcommand)]
enum Commands {
    Create {
        name: PathBuf,
        #[arg(short, long, default_value = "500MB")]
        size: String,
        #[arg(long)]
        password: bool,
        #[arg(long, conflicts_with = "password")]
        no_password: bool,
    },
    Verify {
        path: PathBuf,
        #[arg(long)]
        no_password: bool,
    },
    List {
        path: PathBuf,
        #[arg(long)]
        no_password: bool,
    },
    Mount {
        path: PathBuf,
        mount_point: PathBuf,
        /// Serve the vault over a loopback WebDAV bridge instead of a kernel
        /// filesystem mount (the way macOS mounts without macFUSE).
        #[arg(long)]
        webdav: bool,
        #[arg(long)]
        no_password: bool,
    },
    Defrag {
        path: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        no_password: bool,
    },
    Add {
        path: PathBuf,
        source: PathBuf,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(long)]
        recursive: bool,
        #[arg(long)]
        symlink: bool,
        #[arg(long, requires = "symlink")]
        mount_point: Option<PathBuf>,
        #[arg(long)]
        no_password: bool,
    },
    Extract {
        path: PathBuf,
        file: String,
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        /// Remove the file from the container after a verified extract.
        /// Without this flag the file stays in the container.
        #[arg(long = "move", conflicts_with = "keep")]
        move_out: bool,
        /// Accepted for compatibility. Extract already keeps the file.
        #[arg(long, hide = true)]
        keep: bool,
        #[arg(long)]
        no_password: bool,
    },
    /// Change the container password. Prompts for the new password twice.
    /// Confirming an empty password switches the container to no-password mode.
    /// `--no-password` describes the current container, not the new one.
    Passwd {
        path: PathBuf,
        #[arg(long)]
        no_password: bool,
    },
    /// Grow the container's plaintext capacity by `size` (rounded up to 4 MiB blocks).
    Grow {
        path: PathBuf,
        #[arg(long)]
        by: String,
        #[arg(long)]
        no_password: bool,
    },
    Remove {
        path: PathBuf,
        file: String,
        #[arg(long)]
        no_password: bool,
    },
    Mkdir {
        path: PathBuf,
        dir: String,
        #[arg(long)]
        no_password: bool,
    },
    Rename {
        path: PathBuf,
        old: String,
        new: String,
        #[arg(long)]
        no_password: bool,
    },
}

fn localize_command(mut cmd: clap::Command) -> clap::Command {
    cmd = cmd
        .about(ts!("cli.about"))
        .mut_arg("lang", |arg| arg.help(ts!("cli.arg.lang")));
    cmd = localize_subcommand(cmd, "create", ts!("cli.cmd.create"));
    cmd = localize_subcommand(cmd, "verify", ts!("cli.cmd.verify"));
    cmd = localize_subcommand(cmd, "list", ts!("cli.cmd.list"));
    cmd = localize_subcommand(cmd, "mount", ts!("cli.cmd.mount"));
    cmd = localize_subcommand(cmd, "defrag", ts!("cli.cmd.defrag"));
    cmd = localize_subcommand(cmd, "add", ts!("cli.cmd.add"));
    cmd = localize_subcommand(cmd, "extract", ts!("cli.cmd.extract"));
    cmd = localize_subcommand(cmd, "remove", ts!("cli.cmd.remove"));
    cmd = localize_subcommand(cmd, "mkdir", ts!("cli.cmd.mkdir"));
    cmd = localize_subcommand(cmd, "rename", ts!("cli.cmd.rename"));
    cmd = localize_subcommand(cmd, "passwd", ts!("cli.cmd.passwd"));
    cmd = localize_subcommand(cmd, "grow", ts!("cli.cmd.grow"));
    cmd
}

fn localize_subcommand(cmd: clap::Command, name: &str, about: String) -> clap::Command {
    cmd.mut_subcommand(name, |subcommand| subcommand.about(about))
}

fn prompt_password() -> Result<Zeroizing<String>> {
    prompt_labeled(&ts!("cli.password_prompt"))
}

fn prompt_labeled(prompt: &str) -> Result<Zeroizing<String>> {
    Ok(Zeroizing::new(rpassword::prompt_password(prompt)?))
}

fn prompt_new_password() -> Result<Zeroizing<String>> {
    let first = prompt_labeled(&ts!("cli.password_new_prompt"))?;
    let second = prompt_labeled(&ts!("cli.password_confirm_prompt"))?;
    if first.as_str() != second.as_str() {
        return Err(closed_sesame::SesameError::InvalidInput(
            closed_sesame::i18n::keys::PASSWORD_MISMATCH.into(),
        ));
    }
    Ok(first)
}

fn credential(no_password: bool) -> Result<Option<Zeroizing<String>>> {
    if no_password {
        Ok(None)
    } else {
        prompt_password().map(Some)
    }
}

fn credential_ref(password: &Option<Zeroizing<String>>) -> Credential<'_> {
    password
        .as_deref()
        .map_or(Credential::NoPassword, |password| {
            Credential::Password(password.as_str())
        })
}

fn main() -> ExitCode {
    i18n::init_cli();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}: {}", ts!("common.error_prefix"), error.localized());
            if let Some(hint) = closed_sesame::mount_hints::supplemental_guidance(&error) {
                eprintln!("\n{hint}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let matches = localize_command(Cli::command()).get_matches();
    let cli = Cli::from_arg_matches(&matches).map_err(|error: clap::Error| {
        closed_sesame::SesameError::InvalidInput(error.to_string())
    })?;
    i18n::init(cli.lang);

    match cli.command {
        Commands::Create {
            name,
            size,
            password: _,
            no_password,
        } => {
            let data_size = parse_size(&size)?;
            let password = credential(no_password)?;
            let path = vault::create(name, data_size, credential_ref(&password))?;
            println!(
                "{}",
                ts!(
                    "cli.created",
                    path = path.display().to_string(),
                    size = fmt_size(data_size as u64)
                )
            );
        }
        Commands::Verify { path, no_password } => {
            let password = credential(no_password)?;
            let pages = vault::verify(&path, credential_ref(&password))?;
            println!("{}", ts!("cli.verify_ok", pages = pages));
        }
        Commands::List { path, no_password } => {
            let password = credential(no_password)?;
            let files = vault::list(&path, credential_ref(&password))?;
            if files.is_empty() {
                println!("{}", ts!("cli.empty_container"));
            } else {
                for f in &files {
                    let kind = if f.is_directory { "📁" } else { "📄" };
                    println!("{kind} {:>8}  {}", fmt_size(f.size), f.path);
                }
            }
        }
        Commands::Mount {
            path,
            mount_point,
            webdav,
            no_password,
        } => {
            let password = credential(no_password)?;
            let mut mounted = if webdav {
                let kek = prepare_credentials(&path, credential_ref(&password))?;
                closed_sesame::mount::mount_webdav_with_kek(&kek, &path)?
            } else {
                closed_sesame::mount::check_prerequisites()?;
                closed_sesame::mount::mount_vault(credential_ref(&password), &path, &mount_point)?
            };
            if webdav {
                #[cfg(target_os = "macos")]
                closed_sesame::mount::finder_mount_volume(&mounted.mount_point().to_string_lossy());
            }
            println!(
                "{}",
                ts!(
                    "cli.mounted",
                    path = mounted.mount_point().display().to_string()
                )
            );
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            mounted.unmount()?;
        }
        Commands::Defrag {
            path,
            force,
            no_password,
        } => {
            let password = credential(no_password)?;
            let kek = prepare_credentials(&path, credential_ref(&password))?;
            let stats = defrag::defragment(&path, &kek, force)?;
            if stats.files_moved == 0 {
                println!(
                    "{}",
                    ts!(
                        "cli.defrag_below_threshold",
                        percent = format!("{:.0}", stats.fragmentation_before * 100.0)
                    )
                );
            } else {
                println!(
                    "{}",
                    ts!(
                        "cli.defrag_complete",
                        files = stats.files_moved,
                        pages_before = stats.pages_before,
                        pages_after = stats.pages_after,
                        frag_before = format!("{:.0}", stats.fragmentation_before * 100.0),
                        frag_after = format!("{:.0}", stats.fragmentation_after * 100.0),
                        seconds = format!("{:.1}", stats.duration_secs)
                    )
                );
            }
        }
        Commands::Add {
            path,
            source,
            name,
            recursive,
            symlink,
            mount_point,
            no_password,
        } => {
            let password = credential(no_password)?;
            let cred = credential_ref(&password);
            if symlink && mount_point.is_none() {
                return Err(closed_sesame::SesameError::InvalidInput(
                    keys::MOUNT_POINT_REQUIRED_SYMLINK.into(),
                ));
            }
            let disposition = if symlink {
                AddDisposition::MoveWithSymlink
            } else {
                AddDisposition::Copy
            };
            if recursive {
                if !source.is_dir() {
                    return Err(closed_sesame::SesameError::InvalidInput(
                        keys::RECURSIVE_REQUIRES_DIR.into(),
                    ));
                }
                let credentials = prepare_credentials(&path, cred)?;
                let target_prefix = name.as_deref().map(str::to_owned).or_else(|| {
                    source
                        .file_name()
                        .map(|value| value.to_string_lossy().into_owned())
                });
                let entries = vault::add_dir_with_credentials_and_disposition(
                    &path,
                    &credentials,
                    &source,
                    target_prefix.as_deref(),
                    disposition,
                )?;
                if let Some(mount_point) = mount_point.as_ref() {
                    let mut registry = SymlinkRegistry::default();
                    if let Some(container_dir) = target_prefix.as_deref() {
                        registry.move_dir_with_symlink(&source, mount_point, container_dir)?;
                    } else {
                        for entry in entries.iter().filter(|entry| !entry.is_directory) {
                            if let Some(original) = entry.original_path.as_deref() {
                                registry.move_with_symlink(original, mount_point, &entry.path)?;
                            }
                        }
                    }
                }
                println!(
                    "{}",
                    ts!(
                        "cli.added_entries",
                        count = entries.len(),
                        source = source.display().to_string()
                    )
                );
            } else {
                let entry = vault::add_with_disposition(
                    &path,
                    cred,
                    &source,
                    name.as_deref(),
                    disposition,
                )?;
                println!(
                    "{}",
                    ts!(
                        "cli.added_file",
                        path = entry.path,
                        size = fmt_size(entry.size)
                    )
                );
                if symlink {
                    let mount_point = mount_point.as_ref().expect("checked above");
                    let mut registry = SymlinkRegistry::default();
                    registry.move_with_symlink(&source, mount_point, &entry.path)?;
                    println!(
                        "{}",
                        ts!(
                            "cli.symlink_created",
                            source = source.display().to_string(),
                            target = mount_point.join(&entry.path).display().to_string()
                        )
                    );
                }
            }
        }
        Commands::Extract {
            path,
            file,
            output,
            move_out,
            keep: _,
            no_password,
        } => {
            let password = credential(no_password)?;
            let out_path = vault::extract(&path, credential_ref(&password), &file, &output)?;
            if move_out {
                vault::remove(&path, credential_ref(&password), &file)?;
                println!(
                    "{}",
                    ts!(
                        "cli.extracted_moved",
                        file = file,
                        output = out_path.display().to_string()
                    )
                );
                return Ok(());
            }
            println!(
                "{}",
                ts!(
                    "cli.extracted",
                    file = file,
                    output = out_path.display().to_string()
                )
            );
        }
        Commands::Remove {
            path,
            file,
            no_password,
        } => {
            let password = credential(no_password)?;
            vault::remove(&path, credential_ref(&password), &file)?;
            println!("{}", ts!("cli.removed", file = file));
        }
        Commands::Mkdir {
            path,
            dir,
            no_password,
        } => {
            let password = credential(no_password)?;
            let entry = vault::mkdir(&path, credential_ref(&password), &dir)?;
            println!("{}", ts!("cli.mkdir_created", dir = entry.path));
        }
        Commands::Rename {
            path,
            old,
            new,
            no_password,
        } => {
            let password = credential(no_password)?;
            vault::rename(&path, credential_ref(&password), &old, &new)?;
            println!("{}", ts!("cli.renamed", old = old, new = new));
        }
        Commands::Passwd { path, no_password } => {
            let password = credential(no_password)?;
            let credentials =
                closed_sesame::app::prepare_credentials(&path, credential_ref(&password))?;
            let new_password = prompt_new_password()?;
            if new_password.is_empty() {
                vault::clear_password(&path, &credentials)?;
                println!("{}", ts!("cli.password_cleared"));
            } else {
                vault::change_password(&path, &credentials, new_password.as_str())?;
                println!("{}", ts!("cli.password_changed"));
            }
        }
        Commands::Grow {
            path,
            by,
            no_password,
        } => {
            let password = credential(no_password)?;
            let credentials =
                closed_sesame::app::prepare_credentials(&path, credential_ref(&password))?;
            let additional = closed_sesame::app::parse_size(&by)?;
            let capacity = vault::grow(&path, &credentials, additional as u64)?;
            println!(
                "{}",
                ts!(
                    "cli.grew",
                    capacity = closed_sesame::app::fmt_size(capacity)
                )
            );
        }
    }
    Ok(())
}
