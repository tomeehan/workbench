mod app;
mod db;
mod tui;
mod ui;

use color_eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;

    let mut terminal = tui::init()?;
    let mut app = app::App::new()?;

    while !app.should_quit {
        terminal.draw(|frame| ui::render(&app, frame))?;
        app.handle_events()?;
    }

    tui::restore()?;
    Ok(())
}
