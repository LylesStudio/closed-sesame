//! `sesame-recover` — standalone recovery tool for .sesame containers.
//!
//! Stateless, read-only, never modifies the original .sesame file.

rust_i18n::i18n!("locales", fallback = "en");

macro_rules! ts {
    ($($args:tt)*) => {{
        rust_i18n::t!($($args)*).into_owned()
    }};
}

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use closed_sesame::app::{
    credentials::Credential,
    fmt_size,
    recovery::{self, RecoveryOutcome},
};
use closed_sesame::error::Result;
use closed_sesame::i18n::{self, Locale};
use std::path::PathBuf;
use std::process::ExitCode;
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "sesame-recover", version)]
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
    Extract {
        path: PathBuf,
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        #[arg(long)]
        skip_corrupted: bool,
        #[arg(long)]
        no_password: bool,
    },
}

fn localize_command(mut cmd: clap::Command) -> clap::Command {
    cmd = cmd.mut_arg("lang", |arg| arg.help(ts!("cli.arg.lang")));
    for (name, about) in [
        ("verify", ts!("recover.cmd.verify")),
        ("list", ts!("recover.cmd.list")),
        ("extract", ts!("recover.cmd.extract")),
    ] {
        cmd = cmd.mut_subcommand(name, |subcommand| subcommand.about(about.clone()));
    }
    cmd
}

fn prompt_password() -> Result<Zeroizing<String>> {
    Ok(Zeroizing::new(rpassword::prompt_password(&ts!(
        "cli.password_prompt"
    ))?))
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
        Commands::Verify { path, no_password } => {
            let password = credential(no_password)?;
            let pages = recovery::verify(&path, credential_ref(&password))?;
            println!("{}", ts!("cli.verify_ok", pages = pages));
        }
        Commands::List { path, no_password } => {
            let password = credential(no_password)?;
            let files = recovery::list(&path, credential_ref(&password))?;
            if files.is_empty() {
                println!("{}", ts!("cli.empty_container"));
            } else {
                for f in &files {
                    let kind = if f.is_directory { "📁" } else { "📄" };
                    println!("{kind} {:>8}  {}", fmt_size(f.size), f.path);
                }
            }
        }
        Commands::Extract {
            path,
            output,
            skip_corrupted,
            no_password,
        } => {
            let password = credential(no_password)?;
            let report =
                recovery::extract_all(&path, credential_ref(&password), &output, skip_corrupted)?;
            if report.items.is_empty() {
                println!("{}", ts!("recover.empty_container"));
                return Ok(());
            }
            println!(
                "{}",
                ts!(
                    "recover.extracting",
                    count = report.items.len(),
                    output = output.display().to_string()
                )
            );
            for item in &report.items {
                match &item.outcome {
                    RecoveryOutcome::Extracted(_) => {
                        println!("{}", ts!("recover.extracted_item", path = item.path));
                    }
                    RecoveryOutcome::SkippedCorrupt => {
                        eprintln!("{}", ts!("recover.skipped_item", path = item.path));
                    }
                    RecoveryOutcome::Failed(error) => {
                        eprintln!(
                            "{}",
                            ts!("recover.failed_item", path = item.path, error = error)
                        );
                    }
                }
            }
            println!(
                "{}",
                ts!(
                    "recover.done",
                    extracted = report.extracted(),
                    failed = report.failed(),
                    skipped = report.skipped()
                )
            );
        }
    }
    Ok(())
}
