use crate::config;
use colored::*;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Any number of Vector config files to test. If none are specified the
    /// default config path `/etc/vector/vector.toml` will be targeted.
    paths: Vec<PathBuf>,
}

pub async fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let mut failed_files: Vec<(String, Vec<(String, Vec<String>)>)> = Vec::new();
    let mut inspected_files: Vec<(String, Vec<(String, Vec<String>)>)> = Vec::new();

    let paths = config::process_paths(&opts.paths).unwrap_or_else(|| {
        std::process::exit(exitcode::CONFIG);
    });

    for (i, path) in paths.iter().enumerate() {
        let path_str = path.to_str().unwrap_or("");
        if i > 0 {
            println!();
        }
        println!("Running {} tests", path_str);
        match config::build_unit_tests(path.clone()).await {
            Ok(mut tests) => {
                let mut aggregated_test_errors = Vec::new();
                let mut aggregated_test_inspections = Vec::new();
                tests.iter_mut().for_each(|t| {
                    let (test_inspections, test_errors) = t.run();
                    if !test_inspections.is_empty() {
                        aggregated_test_inspections.push((t.name.clone(), test_inspections));
                    }
                    if !test_errors.is_empty() {
                        println!("test {}: {} ... {}", path_str, t.name, "failed".red());
                        aggregated_test_errors.push((t.name.clone(), test_errors));
                    } else {
                        println!("test {}: {} ... {}", path_str, t.name, "passed".green());
                    }
                });
                if !aggregated_test_inspections.is_empty() {
                    inspected_files.push((path_str.to_owned(), aggregated_test_inspections));
                }
                if !aggregated_test_errors.is_empty() {
                    failed_files.push((path_str.to_owned(), aggregated_test_errors));
                }
                if tests.is_empty() {
                    println!("{}", "No tests found.".yellow());
                }
            }
            Err(errs) => {
                error!(
                    "Failed to execute {} tests:\n{}.",
                    path_str,
                    errs.join("\n")
                );
                return exitcode::CONFIG;
            }
        }
    }

    if !inspected_files.is_empty() {
        println!("\ninspections:");
        for (path, inspections) in inspected_files {
            println!("\n--- {} ---", path);
            for (test_name, inspection) in inspections {
                println!("\ntest '{}':\n", test_name);
                for inspect in inspection {
                    println!("{}\n", inspect);
                }
            }
        }
    }

    if !failed_files.is_empty() {
        println!("\nfailures:");
        for (path, failures) in failed_files {
            println!("\n--- {} ---", path);
            for (test_name, fails) in failures {
                println!("\ntest '{}':\n", test_name);
                for fail in fails {
                    println!("{}\n", fail);
                }
            }
        }
        exitcode::CONFIG
    } else {
        exitcode::OK
    }
}
