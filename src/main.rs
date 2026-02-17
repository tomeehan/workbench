mod ai;
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
                if tmux::is_inside_tmux() {
                    // Inside tmux: switch-client returns immediately, app keeps running
                    let _ = tmux::attach_session(&name);
                    app.refresh_tmux_sessions();
                } else {
                    // Outside tmux: attach blocks until detach
                    tui::restore()?;
                    drop(terminal);

                    let _ = tmux::attach_session(&name);

                    terminal = tui::init()?;
                    app.refresh_tmux_sessions();
                }
            }
        }
    }

    tui::restore()?;
    Ok(())
}
