use std::{process::exit, time::Instant};

use colored::Colorize;

mod clap;
mod display_timings;
mod errors;
mod io;
mod test_server;

use crate::clap::app;
use io::{read_bytes_from_path, read_from_path, write_bytes};

use ::clap::ArgMatches;
use errors::StorkCommandLineError;
use num_format::{Locale, ToFormattedString};
use stork_lib::{build_index, get_output_filename_from_old_style_config, search};

pub type ExitCode = i32;
pub const EXIT_SUCCESS: ExitCode = 0;
pub const EXIT_FAILURE: ExitCode = 1;

type CmdResult = Result<(), StorkCommandLineError>;

fn main() {
    let app_matches = app().get_matches();

    let result = match app_matches.subcommand() {
        ("build", Some(submatches)) => build_handler(submatches, &app_matches),
        ("search", Some(submatches)) => search_handler(submatches, &app_matches),
        ("test", Some(submatches)) => test_handler(submatches, &app_matches),

        // Delete when releasing 2.0.0
        (_, _) => {
            fn print_nudging_string(errant_command: &str) {
                eprintln!("{} The command line interface has been updated: please use `stork {}` instead of `stork --{}`. See `stork --help` for more.", "Warning:".yellow(), errant_command, errant_command)
            }

            if let Some(config_path) = app_matches.value_of("build") {
                // gotta wrap it in a closure so our question marks work
                let wrapper = || -> CmdResult {
                    print_nudging_string("build");
                    let config = read_from_path(config_path)?;
                    let output_path = get_output_filename_from_old_style_config(&config)
                        .ok_or_else(|| {
                            let msg = "You've used the old-style command line interface (`stork --build`) with an index file that is missing an output filename, so Stork can't figure out where to write your index.";
                            StorkCommandLineError::InvalidCommandLineArguments(msg)
                        })?;

                    #[rustfmt::skip]
                    let itr = vec!["stork", "build", "--input", config_path, "--output", &output_path];
                    let global_matches = app().get_matches_from(itr);
                    let submatches = global_matches.subcommand_matches("build").unwrap();
                    build_handler(submatches, &global_matches)
                };
                wrapper()
            } else if let Some(values_iter) = app_matches.values_of("search") {
                print_nudging_string("search");
                let values: Vec<&str> = values_iter.collect();
                let global_matches = app().get_matches_from(vec![
                    "stork", "search", "--input", values[0], "--query", values[1],
                ]);
                let submatches = global_matches.subcommand_matches("search").unwrap();
                search_handler(submatches, &global_matches)
            } else if let Some(input_file) = app_matches.value_of("test") {
                print_nudging_string("test");
                let global_matches =
                    app().get_matches_from(vec!["stork", "test", "--input", input_file]);
                let submatches = global_matches.subcommand_matches("search").unwrap();
                test_handler(submatches, &global_matches)
            } else {
                let _ = app().print_help();
                Ok(())
            }
        }
    };

    if let Err(error) = result {
        eprintln!("{} {}", "Error:".red(), error);
        exit(EXIT_FAILURE);
    }
}

fn build_handler(submatches: &ArgMatches, global_matches: &ArgMatches) -> CmdResult {
    let start_time = Instant::now();

    let config_path = submatches.value_of("config").unwrap();
    let output_path = submatches.value_of("output").unwrap();

    let config = read_from_path(config_path)?;
    let build_output = build_index(&config)?;

    let build_time = Instant::now();

    let bytes_written = write_bytes(output_path, &build_output.bytes)?;

    let end_time = Instant::now();

    eprint!("{}", build_output.description);

    eprintln!(
        "Index built, {} bytes written to {}.",
        bytes_written.to_formatted_string(&Locale::en),
        output_path,
    );

    if global_matches.is_present("timing") {
        eprintln!(
            "{}",
            display_timings![
                (build_time.duration_since(start_time), "to build index"),
                (end_time.duration_since(build_time), "to write file"),
                (end_time.duration_since(start_time), "total")
            ],
        )
    }

    Ok(())
}

fn search_handler(submatches: &ArgMatches, global_matches: &ArgMatches) -> CmdResult {
    let start_time = Instant::now();

    let path = submatches.value_of("index").unwrap();
    let query = submatches.value_of("query").unwrap();

    let index_bytes = read_bytes_from_path(path)?;

    let read_time = Instant::now();

    let results = search(index_bytes, query)?;

    let end_time = Instant::now();

    println!("{}", serde_json::to_string_pretty(&results).unwrap());

    if global_matches.is_present("timing") {
        eprintln!(
            "{}",
            display_timings![
                (read_time.duration_since(start_time), "to read index file"),
                (end_time.duration_since(read_time), "to get search results"),
                (end_time.duration_since(start_time), "total")
            ]
        )
    }

    Ok(())
}

fn test_handler(submatches: &ArgMatches, _global_matches: &ArgMatches) -> CmdResult {
    let port_string = submatches.value_of("port").unwrap();
    let port = port_string
        .parse()
        .map_err(|e| StorkCommandLineError::InvalidPort(port_string.to_string(), e))?;

    if let Some(config_path) = submatches.value_of("config") {
        let config = read_from_path(config_path)?;
        let output = build_index(&config)?;
        test_server::serve(&output.bytes, port).map_err(|_| StorkCommandLineError::ServerError)
    } else if let Some(index_path) = submatches.value_of("index") {
        let index = read_bytes_from_path(index_path)?;
        test_server::serve(&index, port).map_err(|_| StorkCommandLineError::ServerError)
    } else {
        Err(StorkCommandLineError::InvalidCommandLineArguments(
            "Test server requires either --config or --index",
        ))
    }
}
