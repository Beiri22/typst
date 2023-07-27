use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::term::{self, termcolor};
use termcolor::{ColorChoice, StandardStream};
use typst::diag::{Severity, SourceDiagnostic, StrResult};
use typst::doc::Document;
use typst::eval::{eco_format, Tracer};
use typst::World;
use typst_library::prelude::EcoString;

use crate::args::{CompileCommand, DiagnosticFormat, QueryCommand};
use crate::world::SystemWorld;
use crate::{color_stream, set_failed};

/// Execute a compilation command.
pub fn query(command: QueryCommand) -> StrResult<()> {
    let mut world = SystemWorld::new(&CompileCommand{ // Little hack, only 3 fields are used
        font_paths: command.font_paths.clone(),
        input: command.input.clone(),
        root: command.root.clone(),
        output: None,
        flamegraph: None,
        open: None,
        diagnostic_format: command.diagnostic_format.clone(),
        ppi: 0.0,
    })?;
    tracing::info!("Starting querying");

    let start = std::time::Instant::now();
    // Reset everything and ensure that the main file is still present.
    world.reset();
    world.source(world.main()).map_err(|err| err.to_string())?;

    let mut tracer = Tracer::default();

    let result = typst::compile(&world, &mut tracer);
    let duration = start.elapsed();

    let warnings = tracer.warnings();

    match result {
        // Export the PDF / PNG.
        Ok(document) => {
            export(&document, &command)?;

            tracing::info!("Processing succeeded in {duration:?}");

            print_diagnostics(&world, &[], &warnings, command.diagnostic_format)
                .map_err(|_| "failed to print diagnostics")?;
        }

        // Print diagnostics.
        Err(errors) => {
            set_failed();
            tracing::info!("Processing failed");

            print_diagnostics(&world, &errors, &warnings, command.diagnostic_format)
                .map_err(|_| "failed to print diagnostics")?;
        }
    }

    Ok(())
}

fn export(document: &Document, command: &QueryCommand) -> StrResult<()>
{
    let key: EcoString = command.key.clone().into();
    let metadata = document.provided_metadata.get(&key).ok_or("Key not found.")?;

    if command.one {
        if metadata.len()>1{
            Err(format!("One piece of metadata expected, but {} found.", metadata.len()).into())
        }
        else {
            println!("ONE FOUND");
            Ok(())
        }
    }
    else {
        println!("Multiple found");
        println!("{} ",serde_json::to_string(metadata).unwrap());
        println!("{} ",serde_yaml::to_string(metadata).unwrap());
        Ok(())
    }
}

/// Print diagnostic messages to the terminal.
fn print_diagnostics(
    world: &SystemWorld,
    errors: &[SourceDiagnostic],
    warnings: &[SourceDiagnostic],
    diagnostic_format: DiagnosticFormat,
) -> Result<(), codespan_reporting::files::Error> {
    let mut w = match diagnostic_format {
        DiagnosticFormat::Human => color_stream(),
        DiagnosticFormat::Short => StandardStream::stderr(ColorChoice::Never),
    };

    let mut config = term::Config { tab_width: 2, ..Default::default() };
    if diagnostic_format == DiagnosticFormat::Short {
        config.display_style = term::DisplayStyle::Short;
    }

    for diagnostic in warnings.iter().chain(errors.iter()) {
        let diag = match diagnostic.severity {
            Severity::Error => Diagnostic::error(),
            Severity::Warning => Diagnostic::warning(),
        }
        .with_message(diagnostic.message.clone())
        .with_notes(
            diagnostic
                .hints
                .iter()
                .map(|e| (eco_format!("hint: {e}")).into())
                .collect(),
        )
        .with_labels(vec![Label::primary(
            diagnostic.span.id(),
            world.range(diagnostic.span),
        )]);

        term::emit(&mut w, &config, world, &diag)?;

        // Stacktrace-like helper diagnostics.
        for point in &diagnostic.trace {
            let message = point.v.to_string();
            let help = Diagnostic::help().with_message(message).with_labels(vec![
                Label::primary(point.span.id(), world.range(point.span)),
            ]);

            term::emit(&mut w, &config, world, &help)?;
        }
    }

    Ok(())
}