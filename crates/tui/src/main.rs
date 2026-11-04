use std::{error::Error, io, sync::mpsc, time::Duration};

use crossterm::event::{self, Event, KeyEventKind};
use oma_bible_core::{
    bible::Library,
    data_dir,
    storage::{Bookmarks, Settings},
    theme::ThemeCatalog,
};
use ratatui::DefaultTerminal;

mod app;
mod render;

use app::{App, Screen};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let library = Library::load_default();
    let settings = Settings::load();
    let bookmarks = Bookmarks::load();
    let themes = ThemeCatalog::load();
    let (worker_tx, worker_rx) = mpsc::channel();

    let mut app = App::new(library, themes, settings, bookmarks, worker_tx, worker_rx);

    if app.library.translations().is_empty() {
        app.open_download(Screen::Reader);
        app.message = Some(format!(
            "No translations installed. Add one from the Bible API (data: {}).",
            data_dir().display()
        ));
    } else if !app.themes.is_on_omarchy()
        && app.themes.themes().len() > 1
        && app.settings.theme.is_none()
    {
        // Off Omarchy there is no system theme to follow, so show the catalog
        // straight away instead of hiding it behind a key.
        app.open_themes(Screen::Reader);
    }

    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut app);
    ratatui::restore();
    app.save_position();
    result?;

    Ok(())
}

fn run_app(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.quit {
        app.poll_worker();
        terminal.draw(|frame| render::ui(frame, app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
    }

    Ok(())
}
