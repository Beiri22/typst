use std::time::Instant;

use comemo::Track;
use typst::diag::{bail, StrResult};
use typst::eval::{eval_string, EvalMode, Tracer};
use typst::model::{Introspector, Selector};
use typst::World;
use typst_library::prelude::*;

use crate::args::{OutputFormat, QueryCommand};
use crate::compile::print_diagnostics;
use crate::set_failed;
use crate::world::SystemWorld;

/// Execute a query command.
pub fn query(command: QueryCommand) -> StrResult<()> {
    let mut world = SystemWorld::new(&command.common)?;
    tracing::info!("Starting querying");

    let start = Instant::now();
    // Reset everything and ensure that the main file is present.
    world.reset();
    world.source(world.main()).map_err(|err| err.to_string())?;

    let mut tracer = Tracer::default();
    let result = typst::compile(&world, &mut tracer);
    let duration = start.elapsed();
    let warnings = tracer.warnings();

    match result {
        // Print metadata
        Ok(document) => {
            let data = retrieve(&document, &command, &world)?;
            format(&data, &command)?;
            tracing::info!("Processing succeeded in {duration:?}");

            print_diagnostics(&world, &[], &warnings, command.common.diagnostic_format)
                .map_err(|_| "failed to print diagnostics")?;
        }

        // Print diagnostics.
        Err(errors) => {
            set_failed();
            tracing::info!("Processing failed");

            print_diagnostics(
                &world,
                &errors,
                &warnings,
                command.common.diagnostic_format,
            )
            .map_err(|_| "failed to print diagnostics")?;
        }
    }

    Ok(())
}

fn retrieve(
    document: &Document,
    command: &QueryCommand,
    world: &dyn World,
) -> StrResult<Vec<Content>> {
    let selector = eval_string(
        world.track(),
        &command.selector,
        Span::detached(),
        EvalMode::Code,
        Scope::default(),
    )
    .map_err(|_| "error evaluating the selector string")?
    .cast::<Selector>()?;

    Ok(Introspector::new(&document.pages)
        .query(&selector)
        .into_iter()
        .map(|x| x.into_inner())
        .collect::<Vec<_>>())
}

fn format(data: &[Content], command: &QueryCommand) -> StrResult<()> {
    if command.one && data.len() != 1 {
        bail!("one piece of metadata expected, but {} found", data.len())
    }

    let result = match (&command.format, command.one) {
        (OutputFormat::Json, true) => {
            serde_json::to_string_pretty(&data[0]).map_err(|e| e.to_string())?
        }
        (OutputFormat::Yaml, true) => {
            serde_yaml::to_string(&data[0]).map_err(|e| e.to_string())?
        }
        (OutputFormat::Json, false) => {
            serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?
        }
        (OutputFormat::Yaml, false) => {
            serde_yaml::to_string(&data).map_err(|e| e.to_string())?
        }
    };

    println!("{result}");
    Ok(())
}