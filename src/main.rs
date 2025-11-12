mod app;
mod runtime;
mod services;
mod theme;

use app::App;

use std::time::SystemTime;

use clap::Parser;
use clap::builder::TypedValueParser;
use fern::colors::{Color, ColoredLevelConfig};
use log::LevelFilter;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// add more v's to increase verbosity (example: `-vvv`)
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbosity: u8,
    /// changes the log level
    #[arg(
        long = "log-level",
        default_value_t = LevelFilter::Info,
    )]
    log_level: LevelFilter,
}

fn setup_logger(verbosity: u8, log_level: LevelFilter) -> anyhow::Result<()> {
    let mut logger = fern::Dispatch::new().format(move |out, message, record| {
        let date = humantime::format_rfc3339_millis(SystemTime::now());

        let colors = ColoredLevelConfig::new()
            .error(Color::BrightRed)
            .warn(Color::Yellow)
            .debug(Color::BrightCyan)
            .trace(Color::Magenta);

        if record.target().starts_with("aurorashell") {
            if verbosity == 0 {
                out.finish(format_args!(
                    "[{} {}] {}",
                    date,
                    format_args!(
                        "\x1B[{}m{}\x1B[0m",
                        colors.get_color(&record.level()).to_fg_str(),
                        record.level().as_str().to_lowercase()
                    ),
                    message,
                ))
            } else {
                out.finish(format_args!(
                    "[{} {}] ({}:{}L) {}",
                    date,
                    format_args!(
                        "\x1B[{}m{}\x1B[0m",
                        colors.get_color(&record.level()).to_fg_str(),
                        record.level().as_str().to_lowercase()
                    ),
                    record.file().unwrap(),
                    record.line().unwrap(),
                    message,
                ))
            }
        } else {
            out.finish(format_args!(
                "[{} {}] [{}] {}",
                date,
                format_args!(
                    "\x1B[{}m{}\x1B[0m",
                    colors.get_color(&record.level()).to_fg_str(),
                    record.level().as_str().to_lowercase()
                ),
                record.target(),
                message,
            ))
        };
    });

    // log level sets the log level for aurorashell's code while verbosity
    // changes the log level of dependencies, limited by the log level
    // of the aurorashell code

    logger = match verbosity {
        0 => {
            if LevelFilter::Error as usize > log_level as usize {
                logger.level(log_level)
            } else {
                logger.level(LevelFilter::Error)
            }
        }
        1 | 2 => {
            if LevelFilter::Warn as usize > log_level as usize {
                logger.level(log_level)
            } else {
                logger.level(LevelFilter::Warn)
            }
        }
        3 => {
            if LevelFilter::Info as usize > log_level as usize {
                logger.level(log_level)
            } else {
                logger.level(LevelFilter::Info)
            }
        }
        4 => {
            if LevelFilter::Debug as usize > log_level as usize {
                logger.level(log_level)
            } else {
                logger.level(LevelFilter::Debug)
            }
        }
        _5_or_more => {
            if LevelFilter::Trace as usize > log_level as usize {
                logger.level(log_level)
            } else {
                logger.level(LevelFilter::Trace)
            }
        }
    };

    logger
        .level_for("aurorashell", log_level)
        .chain(std::io::stdout())
        .apply()?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    setup_logger(args.verbosity, args.log_level)?;

    log::debug!("debug enabled");
    log::trace!("trace enabled");

    // run app!!! :3
    Ok(iced::daemon(App::title, App::update, App::view)
        .subscription(App::subscription)
        .style(App::style)
        .run_with(App::new)?)
}
