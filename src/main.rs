mod app;
mod db;
mod tmux;
mod tui;
mod ui;

use app::AppAction;
use color_eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;

    let mut terminal = tui::init()?;
    let mut app = app::App::new()?;

    while !app.should_quit {
        terminal.draw(|frame| ui::render(&app, frame))?;
        match app.handle_events()? {
            AppAction::None => {}
            AppAction::AttachTmux(name) => {
                // Restore terminal before attaching to tmux
                tui::restore()?;
                drop(terminal);

                // Attach to tmux session (blocking)
                let _ = tmux::attach_session(&name);

                // Re-initialize terminal after tmux detach
                terminal = tui::init()?;
                app.refresh_tmux_sessions();
            }
        }
    }

    tui::restore()?;
    Ok(())
}
