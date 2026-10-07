mod app;
mod capture;
mod clipboard;
mod config;
mod editor;
mod export;
mod ui;

fn main() -> gtk::glib::ExitCode {
    app::run()
}
