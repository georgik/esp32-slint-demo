fn main() {
    // Run custom linker helper logic.
    linker_be_nice();
    // Add the "linkall.x" linker script as the last argument.
    println!("cargo:rustc-link-arg=-Tlinkall.x");

    // Configure the Slint build: embed resources for the software renderer.
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer);

    // Compile your .slint UI file.
    slint_build::compile_with_config("ui/printerdemo.slint", config).expect("Slint build failed");

    // Print any additional rustc flags that Slint requires.
    slint_build::print_rustc_flags().expect("Failed to print Slint rustc flags");
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        // The first argument (after the executable name) contains the kind of issue.
        let kind = &args[1];
        // The second argument contains a description (e.g. the missing symbol).
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_defmt_timestamp" => {
                    eprintln!();
                    eprintln!("💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`");
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                _ => (),
            },
            // For other kinds such as "missing-lib", exit with an error.
            _ => {
                std::process::exit(1);
            }
        }

        // Exit with success since we handled the specific scenario.
        std::process::exit(0);
    }

    // When no special arguments are passed, print the error-handling script linker argument.
    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
