use oia_app::local_tools::Control;
use oia_app::news::{Dossier, EXAMPLE, Filter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let control = Control::default();
    let mut arguments = std::env::args_os().skip(1);
    let dossier = match arguments.next() {
        Some(path) => Dossier::open(std::path::Path::new(&path), &control)?,
        None => Dossier::parse(EXAMPLE, &control)?,
    };
    let query = arguments
        .next()
        .map(|value| value.into_string())
        .transpose()
        .map_err(|_| "Search text must be UTF-8.")?
        .unwrap_or_default();
    if arguments.next().is_some() {
        return Err("Usage: inspect-news [dossier.json [exact search text]]".into());
    }
    let view = dossier.view(
        &Filter {
            query,
            ..Filter::default()
        },
        &control,
    )?;
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}
